// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
pub mod apiv2;
mod error;
mod token_store;
mod types;

#[cfg(test)]
mod test;

use crate::check_file_sha1_hash;
use crate::obs_id::ObsId;
pub use apiv2::client::{AsvoClient, AsvoClientConfig, JobsFilter, DEFAULT_API_TIMEOUT};
pub use apiv2::AsvoApiError;
pub use error::AsvoError;
pub use token_store::{default_token_cache_path, StoredTokens};
pub use types::{
    AsvoFilesArray, AsvoJob, AsvoJobId, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec,
    Delivery, DownloadOptions, DownloadProgress,
};

use std::env::current_dir;
use std::fs::{rename, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backoff::backoff::Backoff;
use backoff::{Error, ExponentialBackoff, ExponentialBackoffBuilder};
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use sha1::{Digest, Sha1};
use tar::Archive;
use tee_readwrite::TeeReader;

/// The production MWA ASVO host. Callers that do not need a different
/// server (for example a test or development instance) use this as
/// [`AsvoClientConfig::host`].
pub const DEFAULT_ASVO_HOST: &str = "https://asvo.mwatelescope.org:443";

/// The number of bytes in one MiB.
pub const BYTES_PER_MIB: usize = 1024 * 1024;

/// The default [`DownloadOptions::buffer_size`]: 100 MiB.
pub const DEFAULT_DOWNLOAD_BUFFER_SIZE: usize = 100 * BYTES_PER_MIB;

/// The default [`DownloadOptions::retry_duration`]: how long a download
/// keeps retrying transient failures before giving up. Matches
/// `ExponentialBackoff`'s own default (900 s).
pub const DEFAULT_DOWNLOAD_RETRY_DURATION: Duration = Duration::from_secs(900);

/// The longest the library sleeps before it asks
/// [`DownloadOptions::should_stop`] again, while it waits to retry.
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Look up a single job by job ID from the supplied list and download it.
pub(crate) fn download_by_job_id(
    http_client: &Client,
    jobs: AsvoJobVec,
    job_id: AsvoJobId,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let mut jobs = jobs;
    debug!("Attempting to download job {}", job_id);
    jobs.0.retain(|j| j.job_id == job_id);
    match jobs.0.len() {
        0 => Err(AsvoError::NoAsvoJob(job_id)),
        1 => download_job(http_client, &jobs.0[0], opts),
        _ => unreachable!(),
    }
}

/// Look up a single ready job by obsid from the supplied list and download it.
/// Fails if zero, or more than one, ready jobs match the obsid.
pub(crate) fn download_by_obs_id(
    http_client: &Client,
    jobs: AsvoJobVec,
    obs_id: ObsId,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let mut all_jobs = jobs.clone();

    debug!("Attempting to download obsid {}", obs_id);
    let mut ready_jobs = jobs;
    ready_jobs
        .0
        .retain(|j| j.obs_id == obs_id && j.job_state == AsvoJobState::Ready);

    match ready_jobs.0.len() {
        0 => {
            all_jobs.0.retain(|j| j.obs_id == obs_id);
            match all_jobs.0.len() {
                0 => Err(AsvoError::NoObsId(obs_id)),
                _ => Err(AsvoError::NoJobReadyForObsId(obs_id)),
            }
        }
        1 => download_job(http_client, &ready_jobs.0[0], opts),
        _ => Err(AsvoError::TooManyObsIds(obs_id)),
    }
}

/// Download all files for a single job, dispatching by delivery type.
fn download_job(
    http_client: &Client,
    job: &AsvoJob,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    if job.job_state != AsvoJobState::Ready {
        return Err(AsvoError::NotReady {
            job_id: job.job_id,
            job_state: job.job_state.clone(),
        });
    }

    let files = match &job.files {
        None => return Err(AsvoError::NoFiles(job.job_id)),
        Some(f) if f.is_empty() => return Err(AsvoError::NoFiles(job.job_id)),
        Some(f) => f,
    };

    let log_prefix = format!(
        "Job ID {} (obsid: {}) [{}/{}]:",
        job.job_id, job.obs_id, opts.download_number, opts.download_count
    );

    let start_time = Instant::now();

    for f in files {
        match f.r#type {
            Delivery::Acacia => {
                let url = f
                    .url
                    .as_deref()
                    .ok_or(AsvoError::NoUrl { job_id: job.job_id })?;

                debug!("{} Downloading from url {}", log_prefix, url);
                let url_obj = reqwest::Url::parse(url).unwrap();
                let out_path = Path::new(opts.download_dir)
                    .join(url_obj.path_segments().unwrap().next_back().unwrap());

                let op = || {
                    try_download(http_client, url, f, job, &out_path, &log_prefix, opts).map_err(
                        |e| match &e {
                            AsvoError::IO(_) | AsvoError::Interrupted => Error::permanent(e),
                            AsvoError::HttpError { status: 404, .. } => {
                                Error::permanent(AsvoError::Http404Error { job_id: job.job_id })
                            }
                            AsvoError::HttpError {
                                status: 401 | 403, ..
                            } => Error::permanent(e),
                            _ => Error::transient(e),
                        },
                    )
                };

                retry_unless_stopped(download_backoff(opts.retry_duration), op, opts)?;

                let elapsed = start_time.elapsed();
                let elapsed_ms = elapsed.as_millis() as u64;

                let throughput_str = if elapsed_ms == 0 {
                    "N/A".to_string()
                } else {
                    bytesize::ByteSize((f.size * 1000).checked_div(elapsed_ms).unwrap_or_default())
                        .display()
                        .iec()
                        .to_string()
                };

                let duration_str = if elapsed.as_secs() > 60 {
                    format!(
                        "{} min {:.2} s",
                        elapsed.as_secs() / 60,
                        (elapsed.as_millis() as f64 / 1e3) % 60.0
                    )
                } else {
                    format!("{:.3} s", elapsed.as_millis() as f64 / 1e3)
                };

                info!(
                    "{} Completed download of {} in {} ({}/s)",
                    log_prefix,
                    bytesize::ByteSize(f.size).display().iec(),
                    duration_str,
                    throughput_str
                );
            }
            Delivery::Dug => {
                error!(
                    "{} Files for Job are not reachable from the current host. \
                     You will find your job's files on the DUG filesystem.",
                    log_prefix
                );
            }
            Delivery::Scratch => {
                let path = f
                    .path
                    .as_deref()
                    .ok_or(AsvoError::NoPath { job_id: job.job_id })?;
                let path_obj = Path::new(path);
                let folder_name = path_obj
                    .components()
                    .next_back()
                    .unwrap()
                    .as_os_str()
                    .to_str()
                    .unwrap();

                if !path_obj.exists() {
                    error!(
                        "{} Files for Job are not reachable from the current host. \
                         You will find your job's files on the scratch filesystem at Pawsey.",
                        log_prefix
                    );
                } else {
                    info!(
                        "{} Files for Job are reachable from the current host. \
                         Copying to current directory.",
                        log_prefix
                    );
                    let mut current_path = current_dir()?;
                    current_path.push(folder_name);
                    rename(path, current_path)?;
                }
            }
        }
    }

    Ok(())
}

/// Execute a single HTTP file download (Acacia delivery), with optional
/// resume support, stream-untarring, on-the-fly SHA1 hashing, and progress
/// reporting.
fn try_download(
    http_client: &Client,
    url: &str,
    file_info: &AsvoFilesArray,
    job: &AsvoJob,
    out_path: &PathBuf,
    log_prefix: &str,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let buffer_size = opts.buffer_size;

    let mwa_asvo_hash = file_info.sha1.as_deref().unwrap_or_else(|| {
        panic!(
            "{} job does not have an Sha1 hash! \
             Please report this to asvo_support@mwatelescope.org",
            log_prefix
        )
    });

    info!(
        "{} Download starting (type: {}, {})",
        log_prefix,
        job.job_type,
        bytesize::ByteSize(file_info.size).display().iec(),
    );

    let response: reqwest::blocking::Response;
    let mut tee: TeeReader<reqwest::blocking::Response, _>;
    // Set when only part of the file was fetched this time, which changes
    // how the hash has to be checked (see below).
    let mut resumed = false;

    if opts.keep_tar {
        let (mut out_file, mut resume_from) = match prepare_output_file(
            out_path,
            opts.no_resume,
            file_info,
            mwa_asvo_hash,
            job.job_id,
            log_prefix,
        )? {
            OutputTarget::AlreadyDone { reason } => {
                info!("{} {} Skipping {:?}.", log_prefix, reason, out_path);
                report(opts, DownloadProgress::Finished);
                return Ok(());
            }
            OutputTarget::Download { file, offset } => (file, offset),
        };

        report_started(opts, job, log_prefix, file_info.size, resume_from);

        // Only ask for a range when there is something to skip, and ask
        // open-ended: the server knows where the file ends, and a closed
        // range invites off-by-one trouble.
        let headers = if resume_from > 0 {
            let mut headers = HeaderMap::new();
            headers.insert(
                RANGE,
                HeaderValue::from_str(&format!("bytes={resume_from}-"))
                    .expect("a byte range is always a valid header value"),
            );
            Some(headers)
        } else {
            None
        };

        info!(
            "{} {} tar archive {:?}",
            log_prefix,
            if resume_from > 0 {
                "Resuming download of"
            } else {
                "Downloading"
            },
            out_path,
        );

        let http_response = send_checked(http_client, url, headers, log_prefix)?;

        // A server that ignores the range answers 200 with the whole file.
        // Appending that to a partial file would silently corrupt it, so
        // start again from the beginning instead.
        if resume_from > 0 && http_response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            warn!(
                "{} Asked to resume from byte {}, but the server sent the whole file. \
                 Starting again from the beginning.",
                log_prefix, resume_from
            );
            out_file = create_file_logged(out_path, log_prefix)?;
            resume_from = 0;
            report_started(opts, job, log_prefix, file_info.size, resume_from);
        }

        resumed = resume_from > 0;
        response = http_response;
        tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

        copy_with_progress(tee.by_ref(), &mut out_file, buffer_size, opts)?;
    } else {
        let unpack_path = Path::new(opts.download_dir);
        info!(
            "{} Downloading and untarring to {}",
            log_prefix,
            unpack_path.display()
        );

        response = send_checked(http_client, url, None, log_prefix)?;
        tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

        let mut tar = Archive::new(&mut tee);
        tar.set_preserve_mtime(false);

        report_started(opts, job, log_prefix, file_info.size, 0);

        for entry in tar.entries()? {
            let entry = entry.unwrap();
            let entry_path = entry.path()?.to_path_buf();
            let out_full = unpack_path.join(&entry_path);

            if !entry_path.to_str().unwrap().ends_with('/') {
                debug!("{} Writing file {}", log_prefix, out_full.display());
                let mut out_file = create_file_logged(&out_full, log_prefix)?;
                copy_with_progress(
                    BufReader::with_capacity(buffer_size, entry),
                    &mut out_file,
                    buffer_size,
                    opts,
                )?;
            } else if !out_full.exists() {
                debug!("{} Creating directory {:?}", log_prefix, out_full);
                std::fs::create_dir(&out_full).map_err(|e| {
                    error!(
                        "{} Error- cannot create directory {:?}",
                        log_prefix,
                        out_full.display()
                    );
                    AsvoError::IO(e)
                })?;
            } else {
                debug!("{} Directory exists {}", log_prefix, out_full.display());
            }
        }
    }

    // Drain any remaining bytes so the TeeReader's hash covers everything.
    {
        let mut final_bytes = vec![];
        tee.read_to_end(&mut final_bytes)?;
        debug!("{} Read final bytes: {}", log_prefix, final_bytes.len());
    }

    report(opts, DownloadProgress::Finished);

    if opts.hash {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);

        if resumed {
            // The tee only saw the bytes fetched this time, so its hash
            // describes the tail rather than the file. Read the assembled
            // file back instead - slower, but only on a resumed download.
            check_file_sha1_hash(out_path, mwa_asvo_hash, job.job_id)?;
        } else {
            let (_, hasher) = tee.into_inner();
            let hash = format!("{:x}", hasher.finalize());
            debug!("{} Our hash: {}", log_prefix, hash);
            if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
                return Err(AsvoError::HashMismatch {
                    job_id: job.job_id,
                    file: url.to_string(),
                    calculated_hash: hash,
                    expected_hash: mwa_asvo_hash.to_string(),
                });
            }
        }
        info!("{} File matches the MWA ASVO provided hash.", log_prefix);
    }

    Ok(())
}

// --- helpers ---------------------------------------------------------------

/// The retry policy for a download.
///
/// Transient failures (a dropped connection, a 5xx, a hash mismatch) are
/// retried with exponential backoff for `retry_duration`
/// ([`DownloadOptions::retry_duration`]). Zero disables retrying, which is
/// what the test suite uses: a test that deliberately triggers a transient
/// failure would otherwise sit in backoff for fifteen minutes.
/// Whether the caller has asked the download to stop.
fn stop_requested(opts: &DownloadOptions) -> bool {
    opts.should_stop.is_some_and(|should_stop| should_stop())
}

/// Run `op`, and retry it under `backoff` while it fails with a transient
/// error, as `backoff::retry` does. The difference: the wait before each
/// retry is cut into steps of at most [`STOP_CHECK_INTERVAL`], and the
/// retries end with [`AsvoError::Interrupted`] when the caller asks the
/// download to stop. Otherwise a stop could wait for the whole back-off
/// interval, which grows to a minute.
fn retry_unless_stopped<T>(
    mut backoff: ExponentialBackoff,
    mut op: impl FnMut() -> Result<T, Error<AsvoError>>,
    opts: &DownloadOptions,
) -> Result<T, AsvoError> {
    backoff.reset();
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(Error::Permanent(err)) => return Err(err),
            Err(Error::Transient { err, retry_after }) => {
                let Some(wait) = retry_after.or_else(|| backoff.next_backoff()) else {
                    return Err(err);
                };
                debug!("Retrying the download in {:?}: {}", wait, err);
                sleep_unless_stopped(wait, opts)?;
            }
        }
    }
}

/// Sleep for `wait`, and stop early with [`AsvoError::Interrupted`] if the
/// caller asks the download to stop.
fn sleep_unless_stopped(wait: Duration, opts: &DownloadOptions) -> Result<(), AsvoError> {
    if opts.should_stop.is_none() {
        std::thread::sleep(wait);
        return Ok(());
    }
    let deadline = Instant::now() + wait;
    loop {
        if stop_requested(opts) {
            return Err(AsvoError::Interrupted);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        std::thread::sleep((deadline - now).min(STOP_CHECK_INTERVAL));
    }
}

fn download_backoff(retry_duration: Duration) -> backoff::ExponentialBackoff {
    ExponentialBackoffBuilder::new()
        .with_max_elapsed_time(Some(retry_duration))
        .build()
}

/// Send an HTTP GET and check for a successful status code.
fn send_checked(
    http_client: &Client,
    url: &str,
    headers: Option<HeaderMap>,
    log_prefix: &str,
) -> Result<reqwest::blocking::Response, AsvoError> {
    let mut req = http_client.get(url);
    if let Some(h) = headers {
        req = req.headers(h);
    }
    let response = req.send()?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        error!("{} HTTP error {}: {}", log_prefix, status, body);
        return Err(AsvoError::HttpError {
            status: status.as_u16(),
            message: body,
        });
    }
    Ok(response)
}

/// Give `event` to the caller's progress callback, if there is one.
fn report(opts: &DownloadOptions, event: DownloadProgress) {
    if let Some(progress) = opts.progress {
        progress(event);
    }
}

/// Report that a download of `job` starts (or starts again) at `position`
/// bytes of `total_bytes`.
fn report_started(
    opts: &DownloadOptions,
    job: &AsvoJob,
    label: &str,
    total_bytes: u64,
    position: u64,
) {
    report(
        opts,
        DownloadProgress::Started {
            job_id: job.job_id,
            label: label.to_string(),
            total_bytes,
            position,
        },
    );
}

/// Buffered copy from `reader` to `writer`, reporting progress. Before
/// each chunk it asks [`DownloadOptions::should_stop`], and stops with
/// [`AsvoError::Interrupted`] if the caller asks.
fn copy_with_progress(
    reader: impl Read,
    writer: &mut impl Write,
    buffer_size: usize,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let mut buf = BufReader::with_capacity(buffer_size, reader);
    loop {
        if stop_requested(opts) {
            return Err(AsvoError::Interrupted);
        }
        let data = buf.fill_buf()?;
        let len = data.len();
        if len == 0 {
            break;
        }
        writer.write_all(data)?;
        buf.consume(len);
        report(opts, DownloadProgress::Advanced { bytes: len as u64 });
    }
    Ok(())
}

/// Create a file, logging on failure.
fn create_file_logged(path: &Path, log_prefix: &str) -> Result<File, AsvoError> {
    File::create(path).map_err(|e| {
        error!(
            "{} Error- cannot create file {:?}",
            log_prefix,
            path.display()
        );
        AsvoError::IO(e)
    })
}

/// Prepare the output file for a keep-tar download, handling resume and
/// existing-file-with-matching-hash short-circuits.  Returns the open file
/// handle and the byte count already on disk (0 for a fresh download).
/// What the output file on disk means for the download about to happen.
///
/// This used to be signalled by returning an offset equal to the expected
/// file size, with a comment saying the caller should return early - but the
/// caller never checked, so an already-complete file was downloaded again.
/// An explicit outcome makes the "nothing to do" case impossible to miss.
enum OutputTarget {
    /// Nothing to fetch: the file is already complete and verified, or
    /// `--no-resume` means it must be left as it is.
    AlreadyDone { reason: &'static str },

    /// Fetch into this file, starting `offset` bytes in. An `offset` of 0
    /// is a download from the beginning.
    Download { file: File, offset: u64 },
}

fn prepare_output_file(
    out_path: &PathBuf,
    no_resume: bool,
    file_info: &AsvoFilesArray,
    mwa_asvo_hash: &str,
    job_id: AsvoJobId,
    log_prefix: &str,
) -> Result<OutputTarget, AsvoError> {
    if !out_path.try_exists()? {
        return Ok(OutputTarget::Download {
            file: create_file_logged(out_path, log_prefix)?,
            offset: 0,
        });
    }

    // File already exists.
    let file_size_bytes = std::fs::metadata(out_path)?.len();

    if no_resume && file_size_bytes < file_info.size {
        return Ok(OutputTarget::AlreadyDone {
            reason: "Partial file exists, but --no-resume was set.",
        });
    }

    if file_size_bytes == file_info.size {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        match check_file_sha1_hash(out_path, mwa_asvo_hash, job_id) {
            Ok(()) => {
                return Ok(OutputTarget::AlreadyDone {
                    reason: "File exists, is the correct size and matches the MWA ASVO hash.",
                });
            }
            Err(_) => {
                if no_resume {
                    return Ok(OutputTarget::AlreadyDone {
                        reason: "File exists and is the correct size, but its hash does not \
                                 match the MWA ASVO hash, and --no-resume was set.",
                    });
                }
                warn!(
                    "{} File exists and is the correct size, but the hash does not match \
                     the provided MWA ASVO hash. Restarting download...",
                    log_prefix
                );
                return Ok(OutputTarget::Download {
                    file: create_file_logged(out_path, log_prefix)?,
                    offset: 0,
                });
            }
        }
    }

    // A partial file, and resuming is allowed: append to what's there.
    Ok(OutputTarget::Download {
        file: File::options().append(true).open(out_path)?,
        offset: file_size_bytes,
    })
}
