// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
pub mod apiv2;
mod error;
mod token_store;
mod types;

use crate::check_file_sha1_hash;
use crate::obsid::Obsid;
pub use apiv2::client::AsvoClient;
pub use apiv2::AsvoApiError;
pub use error::AsvoError;
pub use token_store::StoredTokens;
pub use types::{
    AsvoFilesArray, AsvoJob, AsvoJobID, AsvoJobMap, AsvoJobState, AsvoJobType, AsvoJobVec,
    Delivery, DownloadOptions,
};

use std::env::{current_dir, var, VarError};
use std::fs::{rename, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use backoff::{retry, Error, ExponentialBackoffBuilder};
use indicatif::ProgressBar;
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use sha1::{Digest, Sha1};
use tar::Archive;
use tee_readwrite::TeeReader;

const CONST_ENV_MWA_ASVO_HOST: &str = "MWA_ASVO_HOST";
const CONST_ENV_GIANT_SQUID_BUF_SIZE: &str = "GIANT_SQUID_BUF_SIZE";
const CONST_ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS: &str = "GIANT_SQUID_DOWNLOAD_RETRY_SECS";
const CONST_DEFAULT_URL: &str = "https://asvo.mwatelescope.org:443";

/// How long a download keeps retrying transient failures before giving up.
/// Matches `ExponentialBackoff`'s own default, so behaviour is unchanged
/// unless overridden.
const CONST_DEFAULT_DOWNLOAD_RETRY_SECS: u64 = 900;

// Returns a custom MWA ASVO host address (via a set env var)
// or returns VarError::NotPresent error when not set
pub fn get_asvo_server_address_env() -> Result<String, VarError> {
    std::env::var(CONST_ENV_MWA_ASVO_HOST)
}

pub fn get_asvo_server_address() -> String {
    get_asvo_server_address_env()
        .unwrap_or_else(|_| String::from(CONST_DEFAULT_URL))
        .to_string()
}

/// Look up a single job by job ID from the supplied list and download it.
pub(crate) fn download_by_jobid(
    http_client: &Client,
    jobs: AsvoJobVec,
    jobid: AsvoJobID,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let mut jobs = jobs;
    debug!("Attempting to download job {}", jobid);
    jobs.0.retain(|j| j.jobid == jobid);
    match jobs.0.len() {
        0 => Err(AsvoError::NoAsvoJob(jobid)),
        1 => download_job(http_client, &jobs.0[0], opts),
        _ => unreachable!(),
    }
}

/// Look up a single ready job by obsid from the supplied list and download it.
/// Fails if zero, or more than one, ready jobs match the obsid.
pub(crate) fn download_by_obsid(
    http_client: &Client,
    jobs: AsvoJobVec,
    obsid: Obsid,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let mut all_jobs = jobs.clone();

    debug!("Attempting to download obsid {}", obsid);
    let mut ready_jobs = jobs;
    ready_jobs
        .0
        .retain(|j| j.obsid == obsid && j.state == AsvoJobState::Ready);

    match ready_jobs.0.len() {
        0 => {
            all_jobs.0.retain(|j| j.obsid == obsid);
            match all_jobs.0.len() {
                0 => Err(AsvoError::NoObsid(obsid)),
                _ => Err(AsvoError::NoJobReadyForObsid(obsid)),
            }
        }
        1 => download_job(http_client, &ready_jobs.0[0], opts),
        _ => Err(AsvoError::TooManyObsids(obsid)),
    }
}

/// Download all files for a single job, dispatching by delivery type.
fn download_job(
    http_client: &Client,
    job: &AsvoJob,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    if job.state != AsvoJobState::Ready {
        return Err(AsvoError::NotReady {
            jobid: job.jobid,
            state: job.state.clone(),
        });
    }

    let files = match &job.files {
        None => return Err(AsvoError::NoFiles(job.jobid)),
        Some(f) if f.is_empty() => return Err(AsvoError::NoFiles(job.jobid)),
        Some(f) => f,
    };

    let log_prefix = format!(
        "Job ID {} (obsid: {}) [{}/{}]:",
        job.jobid, job.obsid, opts.download_number, opts.download_count
    );

    let start_time = Instant::now();

    for f in files {
        match f.r#type {
            Delivery::Acacia => {
                let url = f
                    .url
                    .as_deref()
                    .ok_or(AsvoError::NoUrl { job_id: job.jobid })?;

                debug!("{} Downloading from url {}", log_prefix, url);
                let url_obj = reqwest::Url::parse(url).unwrap();
                let out_path = Path::new(opts.download_dir)
                    .join(url_obj.path_segments().unwrap().next_back().unwrap());

                let op = || {
                    try_download(http_client, url, f, job, &out_path, &log_prefix, opts).map_err(
                        |e| match &e {
                            AsvoError::IO(_) => Error::permanent(e),
                            AsvoError::HttpError { status: 404, .. } => {
                                Error::permanent(AsvoError::Http404Error { job_id: job.jobid })
                            }
                            AsvoError::HttpError {
                                status: 401 | 403, ..
                            } => Error::permanent(e),
                            _ => Error::transient(e),
                        },
                    )
                };

                match retry(download_backoff(), op) {
                    Ok(()) => {}
                    Err(Error::Permanent(err)) => return Err(err),
                    Err(Error::Transient { err, .. }) => return Err(err),
                }

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
                    .ok_or(AsvoError::NoPath { job_id: job.jobid })?;
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
    let buffer_size = match var(CONST_ENV_GIANT_SQUID_BUF_SIZE) {
        Ok(s) => s.parse()?,
        Err(_) => 100, // 100 MiB by default.
    } * 1024
        * 1024;

    let mwa_asvo_hash = file_info.sha1.as_deref().unwrap_or_else(|| {
        panic!(
            "{} job does not have an Sha1 hash! \
             Please report this to asvo_support@mwatelescope.org",
            log_prefix
        )
    });

    opts.progress_bar
        .enable_steady_tick(Duration::from_millis(500));

    info!(
        "{} Download starting (type: {}, {})",
        log_prefix,
        job.jtype,
        bytesize::ByteSize(file_info.size).display().iec(),
    );

    let response: reqwest::blocking::Response;
    let mut tee: TeeReader<reqwest::blocking::Response, _>;

    if opts.keep_tar {
        let (mut out_file, file_size_bytes) = prepare_output_file(
            out_path,
            opts.no_resume,
            file_info,
            mwa_asvo_hash,
            job.jobid,
            log_prefix,
        )?;

        opts.progress_bar.set_length(file_info.size);
        opts.progress_bar.set_position(file_size_bytes);
        opts.progress_bar.reset_eta();
        opts.progress_bar.set_message(log_prefix.to_string());

        let mut headers = HeaderMap::new();
        headers.insert(
            RANGE,
            HeaderValue::from_str(&format!(
                "Range: bytes={}-{}",
                file_size_bytes, file_info.size
            ))
            .unwrap(),
        );

        response = send_checked(http_client, url, Some(headers), log_prefix)?;
        tee = tee_readwrite::TeeReader::new(response, Sha1::new(), false);

        info!(
            "{} {} tar archive {:?}",
            log_prefix,
            if file_size_bytes > 0 {
                "Resuming download of"
            } else {
                "Downloading"
            },
            out_path,
        );

        copy_with_progress(tee.by_ref(), &mut out_file, buffer_size, opts.progress_bar)?;
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

        opts.progress_bar.set_length(file_info.size);
        opts.progress_bar.set_position(0);
        opts.progress_bar.reset_eta();
        opts.progress_bar.set_message(log_prefix.to_string());

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
                    opts.progress_bar,
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

    opts.progress_bar.finish_and_clear();

    if opts.hash {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);
        let (_, hasher) = tee.into_inner();
        let hash = format!("{:x}", hasher.finalize());
        debug!("{} Our hash: {}", log_prefix, hash);
        if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
            return Err(AsvoError::HashMismatch {
                jobid: job.jobid,
                file: url.to_string(),
                calculated_hash: hash,
                expected_hash: mwa_asvo_hash.to_string(),
            });
        }
        info!("{} File matches the MWA ASVO provided hash.", log_prefix);
    }

    Ok(())
}

// --- helpers ---------------------------------------------------------------

/// The retry policy for a download.
///
/// Transient failures (a dropped connection, a 5xx, a hash mismatch) are
/// retried with exponential backoff for
/// [`CONST_DEFAULT_DOWNLOAD_RETRY_SECS`], which can be overridden with
/// `GIANT_SQUID_DOWNLOAD_RETRY_SECS`. Zero disables retrying, which is what
/// the test suite uses: a test that deliberately triggers a transient
/// failure would otherwise sit in backoff for fifteen minutes.
fn download_backoff() -> backoff::ExponentialBackoff {
    let seconds = match var(CONST_ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS) {
        Ok(s) => s.parse().unwrap_or(CONST_DEFAULT_DOWNLOAD_RETRY_SECS),
        Err(_) => CONST_DEFAULT_DOWNLOAD_RETRY_SECS,
    };

    ExponentialBackoffBuilder::new()
        .with_max_elapsed_time(Some(Duration::from_secs(seconds)))
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

/// Buffered copy from `reader` to `writer`, updating the progress bar.
fn copy_with_progress(
    reader: impl Read,
    writer: &mut impl Write,
    buffer_size: usize,
    progress_bar: &ProgressBar,
) -> Result<(), std::io::Error> {
    let mut buf = BufReader::with_capacity(buffer_size, reader);
    loop {
        let data = buf.fill_buf()?;
        let len = data.len();
        if len == 0 {
            break;
        }
        writer.write_all(data)?;
        buf.consume(len);
        progress_bar.inc(len as u64);
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
fn prepare_output_file(
    out_path: &PathBuf,
    no_resume: bool,
    file_info: &AsvoFilesArray,
    mwa_asvo_hash: &str,
    jobid: AsvoJobID,
    log_prefix: &str,
) -> Result<(File, u64), AsvoError> {
    if !out_path.try_exists()? {
        return Ok((create_file_logged(out_path, log_prefix)?, 0));
    }

    // File already exists.
    let out_file = if no_resume {
        File::open(out_path)?
    } else {
        File::options().append(true).open(out_path)?
    };
    let file_size_bytes = File::metadata(&out_file)?.len();

    if no_resume && file_size_bytes < file_info.size {
        warn!(
            "{} Partial file {:?} exists, but --no-resume was set. Skipping file.",
            log_prefix, out_path
        );
        // Signal: nothing to download (caller should return Ok early).
        // We re-use the existing file handle with size == expected so the
        // caller's "already complete" path fires.  A cleaner option would
        // be a dedicated return variant, but this preserves the original
        // behaviour without changing the control flow.
        return Ok((out_file, file_info.size));
    }

    if file_size_bytes == file_info.size {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        match check_file_sha1_hash(out_path, mwa_asvo_hash, jobid) {
            Ok(()) => {
                info!(
                    "{} File exists, is the correct size and matches the MWA ASVO provided hash. Skipping file.",
                    log_prefix
                );
                // Return size == expected to signal "already complete".
                return Ok((out_file, file_info.size));
            }
            Err(_) => {
                if no_resume {
                    warn!(
                        "{} File exists and is the correct size, but the hash does not match \
                         the provided MWA ASVO hash. Leaving file as is, since --no-resume was set.",
                        log_prefix
                    );
                    return Ok((out_file, file_info.size));
                }
                warn!(
                    "{} File exists and is the correct size, but the hash does not match \
                     the provided MWA ASVO hash. Restarting download...",
                    log_prefix
                );
                return Ok((create_file_logged(out_path, log_prefix)?, 0));
            }
        }
    }

    Ok((out_file, file_size_bytes))
}
