// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
pub mod apiv2;
mod env;
mod error;
mod token_store;
mod types;

#[cfg(test)]
mod tests;

use crate::check_file_sha1_hash;
use crate::obs_id::ObsId;
pub use apiv2::client::{
    AsvoClient, AsvoClientConfig, JobQuery, JobsFilter, DEFAULT_API_TIMEOUT,
    ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGE_FROM_JOB, ENDPOINT_IMAGING_JOB, ENDPOINT_JOBS, ENDPOINT_VOLTAGE_JOB,
};
pub use apiv2::AsvoApiError;
pub use env::{
    client_config_from_env, DownloadSettings, ENV_GIANT_SQUID_BUF_SIZE, ENV_GIANT_SQUID_DELIVERY,
    ENV_GIANT_SQUID_DELIVERY_FORMAT, ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, ENV_HOME,
    ENV_MWA_ASVO_API_KEY, ENV_MWA_ASVO_API_TIMEOUT, ENV_MWA_ASVO_HOST,
};
pub use error::AsvoError;
pub use token_store::{default_token_cache_path, StoredTokens};
pub use types::{
    AsvoFilesArray, AsvoJob, AsvoJobId, AsvoJobMap, AsvoJobProduct, AsvoJobState, AsvoJobType,
    AsvoJobVec, Delivery, DownloadOptions, DownloadProgress,
};

use std::cell::RefCell;
use std::env::current_dir;
use std::fmt;
use std::fs::{rename, File};
use std::io::{self, BufRead, BufReader, Read, Write};
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

/// The default number of downloads that the `giant-squid` commands run at
/// the same time.
pub const DEFAULT_CONCURRENT_DOWNLOADS: usize = 4;

/// The time between two job list requests while waiting for jobs, in the
/// `giant-squid` commands.
pub const WAIT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// How long the `giant-squid` commands wait before the first job list request
/// of a wait, so that the user's queue is hopefully current.
pub const WAIT_INITIAL_DELAY: Duration = Duration::from_secs(1);

/// The longest the library sleeps before it asks
/// [`DownloadOptions::should_stop`] again, while it waits to retry.
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// The size of a tar block. Every tar header is one block, and the data of
/// each member is padded with zeros to a whole number of blocks.
const TAR_BLOCK_SIZE: u64 = 512;

/// The largest byte range that one request fetches while giant-squid looks
/// for files from an earlier stream-untar download (see
/// [`untar_checkpoint_from_disk`]). One range usually holds the padding of a
/// member and all the headers of the next.
const EARLIER_FILES_WINDOW: u64 = 64 * 1024;

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

    let files = match &job.product {
        None => return Err(AsvoError::NoFiles(job.job_id)),
        Some(p) if p.files.is_empty() => return Err(AsvoError::NoFiles(job.job_id)),
        Some(p) => &p.files,
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

                // Shared by the attempts at this file, so that a retry
                // carries on from what an earlier attempt wrote.
                let mut retry_state = RetryState::default();

                // A stream-untar download can also carry on from the files
                // that an earlier run wrote. This check is made once, before
                // the first attempt: after a hash mismatch the retry must
                // fetch the whole archive, not use the same files again.
                if !opts.keep_tar && !opts.no_resume {
                    retry_state.untar_checkpoint = untar_checkpoint_from_disk(
                        http_client,
                        url,
                        f,
                        job.job_id,
                        &log_prefix,
                        opts,
                    )?;
                }

                #[allow(clippy::result_large_err)]
                let op = || {
                    try_download(
                        http_client,
                        url,
                        f,
                        job,
                        &out_path,
                        &log_prefix,
                        opts,
                        &mut retry_state,
                    )
                    .map_err(|e| retry_class(e, job.job_id))
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

/// Sort the error of one download attempt into a failure that the retry
/// loop gives up on (permanent) or tries again (transient).
///
/// An IO error is permanent, because it is usually a problem with the local
/// disk, except when it came from reading the download itself (see
/// [`NetworkReader`]): a dropped connection is worth another attempt.
#[allow(clippy::result_large_err)]
fn retry_class(e: AsvoError, job_id: AsvoJobId) -> Error<AsvoError> {
    match &e {
        AsvoError::IO(io_error) if is_network_read_error(io_error) => Error::transient(e),
        AsvoError::IO(_) | AsvoError::Interrupted => Error::permanent(e),
        AsvoError::HttpError { status: 404, .. } => {
            Error::permanent(AsvoError::Http404Error { job_id })
        }
        AsvoError::HttpError {
            status: 401 | 403, ..
        } => Error::permanent(e),
        _ => Error::transient(e),
    }
}

/// What the attempts at one file share, so that a retry carries on from
/// what an earlier attempt wrote instead of starting again.
#[derive(Default)]
struct RetryState {
    /// An earlier attempt wrote to the keep-tar output file. A partial file
    /// is then this download's own, so it is resumed even when
    /// [`DownloadOptions::no_resume`] is set: that option is about partial
    /// files that were there before the download started.
    wrote_tar: bool,

    /// Where a stream-untar retry carries on. `None` starts from the
    /// beginning of the archive.
    untar_checkpoint: Option<UntarCheckpoint>,
}

/// Execute a single HTTP file download (Acacia delivery), with optional
/// resume support, stream-untarring, on-the-fly SHA1 hashing, and progress
/// reporting.
#[allow(clippy::too_many_arguments)]
fn try_download(
    http_client: &Client,
    url: &str,
    file_info: &AsvoFilesArray,
    job: &AsvoJob,
    out_path: &PathBuf,
    log_prefix: &str,
    opts: &DownloadOptions,
    retry_state: &mut RetryState,
) -> Result<(), AsvoError> {
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

    if !opts.keep_tar {
        return try_download_untar(
            http_client,
            url,
            file_info,
            job.job_id,
            out_path,
            log_prefix,
            opts,
            mwa_asvo_hash,
            &mut retry_state.untar_checkpoint,
        );
    }

    let (mut out_file, mut resume_from) = match prepare_output_file(
        out_path,
        opts.no_resume && !retry_state.wrote_tar,
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
    retry_state.wrote_tar = true;

    report_started(opts, job.job_id, log_prefix, file_info.size, resume_from);

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

    let http_response = send_checked(http_client, url, range_from(resume_from), log_prefix)?;

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
        report_started(opts, job.job_id, log_prefix, file_info.size, resume_from);
    }

    // Set when only part of the file was fetched this time, which changes
    // how the hash has to be checked (see below).
    let resumed = resume_from > 0;
    let mut tee = TeeReader::new(response_reader(http_response), Sha1::new(), false);

    copy_with_progress(tee.by_ref(), &mut out_file, opts.buffer_size, opts)?;

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

// --- stream-untar ----------------------------------------------------------

/// Where a stream-untar download can carry on after a failed attempt.
///
/// It is taken when the tar parser gives a member. At that moment the
/// parser has read the member's headers and none of its data, so the hasher
/// covers exactly the archive bytes before the member's data. A retry adds
/// the member bytes that are already on disk (see [`resume_point`]) and
/// fetches only the rest of the archive.
#[derive(Clone)]
struct UntarCheckpoint {
    /// The archive offset of the member's first data byte.
    data_pos: u64,
    /// The SHA1 of the archive bytes before `data_pos`.
    hasher: Sha1,
    /// The file that the member is written to, or `None` for a member that
    /// is not written to a file (a directory).
    out_path: Option<PathBuf>,
    /// The size of the member's data, in bytes.
    size: u64,
}

/// Where one stream-untar attempt starts.
struct ResumePoint {
    /// The archive offset to fetch from.
    start: u64,
    /// The SHA1 of the archive bytes before `start`.
    hasher: Sha1,
    /// The rest of the member that a failed attempt was in, or `None` when
    /// the attempt starts at the beginning of the archive.
    member: Option<MemberTail>,
}

impl ResumePoint {
    /// An attempt that fetches the whole archive.
    fn from_start() -> Self {
        Self {
            start: 0,
            hasher: Sha1::new(),
            member: None,
        }
    }
}

/// The part of a member that a failed attempt did not write.
struct MemberTail {
    /// The member's file, open for append, or `None` for a member that is
    /// not written to a file.
    file: Option<File>,
    /// The member's data bytes still to fetch.
    remaining: u64,
    /// The size of the member's data, which sets the size of its padding.
    size: u64,
}

/// Download a tar file and unpack it while it arrives (stream-untar), into
/// [`DownloadOptions::download_dir`].
///
/// After a failure, the files already written stay on disk, and
/// `checkpoint` tells the next attempt where to carry on. That attempt asks
/// the server only for the rest of the archive, so a retry after a dropped
/// connection does not fetch or write the earlier members again. The SHA1
/// still covers the whole archive: the bytes that are not fetched again are
/// read back from disk.
#[allow(clippy::too_many_arguments)]
fn try_download_untar(
    http_client: &Client,
    url: &str,
    file_info: &AsvoFilesArray,
    job_id: AsvoJobId,
    out_path: &Path,
    log_prefix: &str,
    opts: &DownloadOptions,
    mwa_asvo_hash: &str,
    checkpoint: &mut Option<UntarCheckpoint>,
) -> Result<(), AsvoError> {
    let unpack_path = Path::new(opts.download_dir);

    let mut resume = match checkpoint.as_ref() {
        Some(cp) => resume_point(cp, opts.hash, log_prefix)?,
        None => ResumePoint::from_start(),
    };

    if resume.start > 0 {
        info!(
            "{} Resuming the download and untar to {} at byte {} of {}",
            log_prefix,
            unpack_path.display(),
            resume.start,
            file_info.size
        );
    } else {
        info!(
            "{} Downloading and untarring to {}",
            log_prefix,
            unpack_path.display()
        );
    }

    let response = send_checked(http_client, url, range_from(resume.start), log_prefix)?;

    // A server that ignores the range answers 200 with the whole archive.
    // Start again from the beginning: the members are written again, from
    // the start of the archive.
    if resume.start > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        warn!(
            "{} Asked to resume from byte {}, but the server sent the whole file. \
             Starting again from the beginning.",
            log_prefix, resume.start
        );
        resume = ResumePoint::from_start();
        *checkpoint = None;
    }

    report_started(opts, job_id, log_prefix, file_info.size, resume.start);

    let hasher = untar_stream(
        response_reader(response),
        resume,
        unpack_path,
        log_prefix,
        opts,
        checkpoint,
    )?;

    report(opts, DownloadProgress::Finished);

    if opts.hash {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);
        let hash = format!("{:x}", hasher.finalize());
        debug!("{} Our hash: {}", log_prefix, hash);
        if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
            // The bytes behind the checkpoint are now suspect, so the retry
            // must fetch the whole archive again.
            *checkpoint = None;
            return Err(AsvoError::HashMismatch {
                job_id,
                file: url.to_string(),
                calculated_hash: hash,
                expected_hash: mwa_asvo_hash.to_string(),
            });
        }
        info!("{} File matches the MWA ASVO provided hash.", log_prefix);
    }

    *checkpoint = None;
    Ok(())
}

/// Unpack the archive bytes from `source` into `unpack_path`, and return
/// the SHA1 of the whole archive.
///
/// `source` gives the archive from `resume.start` on. If `resume` is inside
/// a member, the rest of that member is written first. `checkpoint` is set
/// each time a member starts, so that it always tells a next attempt where
/// to carry on.
fn untar_stream(
    source: impl Read,
    resume: ResumePoint,
    unpack_path: &Path,
    log_prefix: &str,
    opts: &DownloadOptions,
    checkpoint: &mut Option<UntarCheckpoint>,
) -> Result<Sha1, AsvoError> {
    let buffer_size = opts.buffer_size;
    let hasher = RefCell::new(resume.hasher);
    let mut reader = HashingReader {
        inner: source,
        hasher: &hasher,
        position: resume.start,
    };

    if let Some(tail) = resume.member {
        // Finish the member that the failed attempt was in, then pass over
        // its padding, so that the tar parser starts at a header.
        let mut data = (&mut reader).take(tail.remaining);
        match tail.file {
            Some(mut file) => copy_with_progress(&mut data, &mut file, buffer_size, opts)?,
            None => {
                io::copy(&mut data, &mut io::sink())?;
            }
        }
        let data_missing = data.limit();
        let mut padding = (&mut reader).take(tar_padding(tail.size));
        io::copy(&mut padding, &mut io::sink())?;
        if data_missing > 0 || padding.limit() > 0 {
            return Err(network_error(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The download ended inside a tar member",
            ))
            .into());
        }
    }

    // The tar parser counts its positions from where it starts reading.
    let base = reader.position;
    {
        let mut tar = Archive::new(&mut reader);
        tar.set_preserve_mtime(false);

        for entry in tar.entries()? {
            let entry = entry?;
            let entry_path = entry.path()?.to_path_buf();
            let out_full = unpack_path.join(&entry_path);
            let is_dir = entry_path.to_str().unwrap().ends_with('/');

            *checkpoint = Some(UntarCheckpoint {
                data_pos: base + entry.raw_file_position(),
                hasher: hasher.borrow().clone(),
                out_path: (!is_dir).then(|| out_full.clone()),
                size: entry.size(),
            });

            if !is_dir {
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

    // Read the rest of the archive (its end-of-archive blocks), so that the
    // hash covers all of it.
    let final_bytes = io::copy(&mut reader, &mut io::sink())?;
    debug!("{} Read final bytes: {}", log_prefix, final_bytes);
    drop(reader);

    Ok(hasher.into_inner())
}

/// Find where the attempt after `checkpoint` starts: after the bytes of the
/// checkpoint's member that are already on disk.
///
/// When `rehash` is set, those bytes are read back from disk into the
/// hasher, so that the SHA1 still covers the whole archive. A file that is
/// larger than its member cannot be this download's own, so the attempt
/// then starts again from the beginning of the archive.
fn resume_point(
    cp: &UntarCheckpoint,
    rehash: bool,
    log_prefix: &str,
) -> Result<ResumePoint, AsvoError> {
    let mut hasher = cp.hasher.clone();
    let (on_disk, file) = match &cp.out_path {
        None => (0, None),
        Some(path) => {
            let on_disk = match std::fs::metadata(path) {
                Ok(metadata) => metadata.len(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
                Err(e) => return Err(e.into()),
            };
            if on_disk > cp.size {
                warn!(
                    "{} {:?} is larger than the tar member it is written from. \
                     Starting again from the beginning.",
                    log_prefix, path
                );
                return Ok(ResumePoint::from_start());
            }
            if rehash && on_disk > 0 {
                hash_file_prefix(path, on_disk, &mut hasher)?;
            }
            let file = File::options().create(true).append(true).open(path)?;
            (on_disk, Some(file))
        }
    };

    Ok(ResumePoint {
        start: cp.data_pos + on_disk,
        hasher,
        member: Some(MemberTail {
            file,
            remaining: cp.size - on_disk,
            size: cp.size,
        }),
    })
}

/// Add the first `len` bytes of the file at `path` to `hasher`.
fn hash_file_prefix(path: &Path, len: u64, hasher: &mut Sha1) -> Result<(), AsvoError> {
    let copied = io::copy(&mut File::open(path)?.take(len), hasher)?;
    if copied < len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("{} became shorter while it was read", path.display()),
        )
        .into());
    }
    Ok(())
}

/// The number of padding bytes after a tar member with `size` data bytes,
/// which fill the member's last block.
fn tar_padding(size: u64) -> u64 {
    (TAR_BLOCK_SIZE - size % TAR_BLOCK_SIZE) % TAR_BLOCK_SIZE
}

/// A reader that adds each byte it reads to a shared SHA1 and counts its
/// archive position. The hasher is shared so that a checkpoint can copy its
/// state while the tar parser holds the reader.
struct HashingReader<'h, R> {
    inner: R,
    hasher: &'h RefCell<Sha1>,
    /// The archive offset of the next byte.
    position: u64,
}

impl<R: Read> Read for HashingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.borrow_mut().update(&buf[..n]);
        self.position += n as u64;
        Ok(n)
    }
}

// --- files from an earlier stream-untar download ---------------------------

/// Look for the members of the archive that an earlier run already unpacked
/// into [`DownloadOptions::download_dir`], and return a checkpoint after them,
/// so that the download fetches only the rest of the archive.
///
/// The check walks the archive in order. A member counts as on disk when its
/// file (or directory) exists and the file has the member's size. The walk
/// stops at the first member that is not on disk. That member can be partly
/// on disk; the download then carries on inside it (see [`resume_point`]).
///
/// The SHA1 still covers the whole archive. The headers and padding come
/// from the server, in byte ranges of at most [`EARLIER_FILES_WINDOW`] bytes.
/// The member data that is on disk is read from the files. A file that has
/// the right size but the wrong contents makes the hash check fail, and the
/// retry then fetches the whole archive. When [`DownloadOptions::hash`] is
/// not set, the files are not read and are used on their size alone.
///
/// Returns `Ok(None)` when there is nothing to carry on from: the download
/// directory is empty, the first member is not on disk, or the check failed
/// (for example, the server does not answer byte range requests). A failed
/// check is not an error: the download fetches the whole archive, as it
/// would without the check. Only a stop that the caller asked for
/// ([`AsvoError::Interrupted`]) is an error.
fn untar_checkpoint_from_disk(
    http_client: &Client,
    url: &str,
    file_info: &AsvoFilesArray,
    job_id: AsvoJobId,
    log_prefix: &str,
    opts: &DownloadOptions,
) -> Result<Option<UntarCheckpoint>, AsvoError> {
    // An empty directory cannot hold files from an earlier run, so it is
    // not worth a request to the server.
    let dir_is_empty = std::fs::read_dir(opts.download_dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true);
    if dir_is_empty {
        return Ok(None);
    }

    match find_files_on_disk(http_client, url, file_info, job_id, log_prefix, opts) {
        Ok(checkpoint) => Ok(checkpoint),
        Err(AsvoError::Interrupted) => Err(AsvoError::Interrupted),
        Err(e) => {
            warn!(
                "{} Could not check for files from an earlier download ({}). \
                 Downloading the whole archive.",
                log_prefix, e
            );
            Ok(None)
        }
    }
}

/// The walk of [`untar_checkpoint_from_disk`], with every failure returned.
fn find_files_on_disk(
    http_client: &Client,
    url: &str,
    file_info: &AsvoFilesArray,
    job_id: AsvoJobId,
    log_prefix: &str,
    opts: &DownloadOptions,
) -> Result<Option<UntarCheckpoint>, AsvoError> {
    let unpack_path = Path::new(opts.download_dir);
    let hasher = RefCell::new(Sha1::new());
    let local = RefCell::new(None);
    let mut reader = HashingReader {
        inner: SpliceReader {
            http_client,
            url,
            log_prefix,
            archive_size: file_info.size,
            position: 0,
            window_start: 0,
            window: Vec::new(),
            local: &local,
        },
        hasher: &hasher,
        position: 0,
    };

    let mut checkpoint = None;
    let mut files_on_disk: u64 = 0;
    let mut bytes_on_disk: u64 = 0;
    {
        let mut tar = Archive::new(&mut reader);
        for entry in tar.entries()? {
            let entry = entry?;
            let entry_path = entry.path()?.to_path_buf();
            let out_full = unpack_path.join(&entry_path);
            let is_dir = entry_path.to_str().unwrap().ends_with('/');
            let size = entry.size();
            let data_pos = entry.raw_file_position();

            checkpoint = Some(UntarCheckpoint {
                data_pos,
                hasher: hasher.borrow().clone(),
                out_path: (!is_dir).then(|| out_full.clone()),
                size,
            });

            let on_disk = if is_dir {
                out_full.is_dir()
            } else {
                std::fs::metadata(&out_full)
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() == size)
            };
            if !on_disk {
                break;
            }

            debug!("{} Already on disk: {}", log_prefix, out_full.display());
            if files_on_disk == 0 {
                report_started(opts, job_id, log_prefix, file_info.size, 0);
            }
            files_on_disk += 1;
            if !is_dir && size > 0 {
                // The member's data comes from its file (or is skipped, when
                // there is no hash to check).
                *local.borrow_mut() = Some(LocalData {
                    start: data_pos,
                    len: size,
                    file: if opts.hash {
                        Some(File::open(&out_full)?)
                    } else {
                        None
                    },
                });
                copy_with_progress(entry, &mut io::sink(), opts.buffer_size, opts)?;
                bytes_on_disk += size;
            }
        }
    }

    let Some(checkpoint) = checkpoint else {
        return Ok(None);
    };

    // Nothing to carry on from: the first member is not on disk, and no part
    // of it either.
    if files_on_disk == 0 {
        let partly_on_disk = checkpoint
            .out_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .is_some_and(|metadata| metadata.is_file() && metadata.len() > 0);
        if !partly_on_disk {
            return Ok(None);
        }
    }

    if files_on_disk > 0 {
        info!(
            "{} Found {} files ({}) from an earlier download in {}. \
             Only the rest of the archive is fetched.",
            log_prefix,
            files_on_disk,
            bytesize::ByteSize(bytes_on_disk).display().iec(),
            unpack_path.display()
        );
    }
    Ok(Some(checkpoint))
}

/// Where [`SpliceReader`] reads a member's data from, instead of the server.
struct LocalData {
    /// The archive offset of the member's first data byte.
    start: u64,
    /// The size of the member's data.
    len: u64,
    /// The member's file, read from its start, or `None` to give zeros: the
    /// bytes are then only counted, because there is no hash to check.
    file: Option<File>,
}

/// A reader over an archive that is partly unpacked on disk. The data of the
/// member set in `local` comes from disk; every other byte comes from the
/// server, fetched in closed byte ranges (see [`EARLIER_FILES_WINDOW`]).
struct SpliceReader<'a> {
    http_client: &'a Client,
    url: &'a str,
    log_prefix: &'a str,
    /// The size of the archive, in bytes.
    archive_size: u64,
    /// The archive offset of the next byte.
    position: u64,
    /// The archive offset of `window`.
    window_start: u64,
    /// The bytes of the last range fetched from the server.
    window: Vec<u8>,
    /// The member data that comes from disk, set by the caller.
    local: &'a RefCell<Option<LocalData>>,
}

impl SpliceReader<'_> {
    /// Read from the member data on disk, if `position` is in it.
    fn read_local(&mut self, buf: &mut [u8]) -> Option<io::Result<usize>> {
        let mut local = self.local.borrow_mut();
        let data = local.as_mut()?;
        let end = data.start + data.len;
        if self.position < data.start || self.position >= end {
            return None;
        }
        let want = buf.len().min((end - self.position) as usize);
        let result = match data.file.as_mut() {
            Some(file) => match file.read(&mut buf[..want]) {
                Ok(0) => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "a file on disk became shorter while it was read",
                )),
                other => other,
            },
            None => {
                buf[..want].fill(0);
                Ok(want)
            }
        };
        Some(result)
    }

    /// Fetch the range of the archive that starts at `position`.
    fn fetch_window(&mut self) -> io::Result<()> {
        let end = (self.position + EARLIER_FILES_WINDOW).min(self.archive_size);
        let mut headers = HeaderMap::new();
        headers.insert(
            RANGE,
            HeaderValue::from_str(&format!("bytes={}-{}", self.position, end - 1))
                .expect("a byte range is always a valid header value"),
        );
        let response = send_checked(self.http_client, self.url, Some(headers), self.log_prefix)
            .map_err(|e| io::Error::other(e.to_string()))?;
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::other(
                "the server does not answer byte range requests",
            ));
        }
        let bytes = response
            .bytes()
            .map_err(|e| io::Error::other(e.to_string()))?;
        if bytes.len() as u64 != end - self.position {
            return Err(io::Error::other(format!(
                "asked for {} bytes from byte {}, but the server sent {}",
                end - self.position,
                self.position,
                bytes.len()
            )));
        }
        self.window_start = self.position;
        self.window = bytes.to_vec();
        Ok(())
    }
}

impl Read for SpliceReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.position >= self.archive_size {
            return Ok(0);
        }
        let n = match self.read_local(buf) {
            Some(result) => result?,
            None => {
                let window_end = self.window_start + self.window.len() as u64;
                if self.position < self.window_start || self.position >= window_end {
                    self.fetch_window()?;
                }
                let offset = (self.position - self.window_start) as usize;
                let n = buf.len().min(self.window.len() - offset);
                buf[..n].copy_from_slice(&self.window[offset..offset + n]);
                n
            }
        };
        self.position += n as u64;
        Ok(n)
    }
}

// --- network reads ---------------------------------------------------------

/// A reader over a download that marks its read errors as network errors
/// (see [`is_network_read_error`]), so that the retry loop can tell a
/// dropped connection from a local disk error.
///
/// A download that ends before its `Content-Length` is also a network
/// error, and not a short file.
struct NetworkReader<R> {
    inner: R,
    /// The bytes still to come, from the response's `Content-Length`, or
    /// `None` when the server gave no length.
    remaining: Option<u64>,
}

impl<R> NetworkReader<R> {
    fn new(inner: R, expected_len: Option<u64>) -> Self {
        Self {
            inner,
            remaining: expected_len,
        }
    }
}

impl<R: Read> Read for NetworkReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buf) {
            Ok(0) if !buf.is_empty() && self.remaining.is_some_and(|left| left > 0) => {
                Err(network_error(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "The connection closed with {} bytes of the download still to come",
                        self.remaining.unwrap_or_default()
                    ),
                )))
            }
            Ok(n) => {
                if let Some(left) = self.remaining.as_mut() {
                    *left = left.saturating_sub(n as u64);
                }
                Ok(n)
            }
            Err(e) => Err(network_error(e)),
        }
    }
}

/// A [`NetworkReader`] over the body of `response`.
fn response_reader(
    response: reqwest::blocking::Response,
) -> NetworkReader<reqwest::blocking::Response> {
    let expected_len = response.content_length();
    NetworkReader::new(response, expected_len)
}

/// Marks an IO error as a failure to read the download from the server. It
/// shows as the error that it wraps.
#[derive(Debug)]
struct NetworkReadError(io::Error);

impl fmt::Display for NetworkReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for NetworkReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

/// Mark `e` as a failure to read the download. The error kind stays the
/// same.
fn network_error(e: io::Error) -> io::Error {
    io::Error::new(e.kind(), NetworkReadError(e))
}

/// Whether `e` is a failure to read the download (see [`network_error`]).
fn is_network_read_error(e: &io::Error) -> bool {
    e.get_ref()
        .is_some_and(|inner| inner.is::<NetworkReadError>())
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

/// A `Range` header that asks for the file from byte `offset` on, or `None`
/// (the whole file) when `offset` is 0.
///
/// The range is open-ended: the server knows where the file ends, and a
/// closed range invites off-by-one trouble.
fn range_from(offset: u64) -> Option<HeaderMap> {
    (offset > 0).then(|| {
        let mut headers = HeaderMap::new();
        headers.insert(
            RANGE,
            HeaderValue::from_str(&format!("bytes={offset}-"))
                .expect("a byte range is always a valid header value"),
        );
        headers
    })
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

/// Report that a download for job `job_id` starts (or starts again) at `position`
/// bytes of `total_bytes`.
fn report_started(
    opts: &DownloadOptions,
    job_id: AsvoJobId,
    label: &str,
    total_bytes: u64,
    position: u64,
) {
    report(
        opts,
        DownloadProgress::Started {
            job_id,
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
