// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Downloading the files of MWA ASVO jobs: the plain (keep-tar) and the
//! stream-untar transfer, resume, retries, the hash check and the resume
//! file.

#[cfg(test)]
mod tests;

use crate::check_file_sha1_hash;
use crate::helpers::{hash_reader, to_hex};
use crate::obs_id::ObsId;

use super::apiv2::openapi::Type as FileType;
use super::{AsvoError, AsvoJob, AsvoJobId, AsvoJobVec, JobFile, JobState};

use std::cell::{Cell, RefCell};
use std::env::current_dir;
use std::fmt;
use std::fs::{rename, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use backoff::backoff::Backoff;
use backoff::{Error, ExponentialBackoff, ExponentialBackoffBuilder};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use serde::{Deserialize, Serialize};
use sha1::digest::common::hazmat::{SerializableState, SerializedState};
use sha1::{Digest, Sha1};
use tar::Archive;

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

/// A download progress event, given to [`DownloadOptions::progress`].
///
/// For each file, the library sends one or more `Started` events, zero or
/// more `Advanced` events, then one `Finished` event. A second `Started`
/// for the same file means the download restarted (for example, the server
/// did not honour a resume request), so the caller resets its count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadProgress {
    /// A file download starts, or starts again.
    Started {
        /// The MWA ASVO job ID.
        job_id: AsvoJobId,
        /// A human-readable label for the download, for example
        /// `Job ID 123 (obsid: 1234567890) [1/2]:`.
        label: String,
        /// The size of the file in bytes.
        total_bytes: u64,
        /// The number of bytes already on disk (non-zero for a resumed
        /// download).
        position: u64,
    },
    /// `bytes` more bytes were written.
    Advanced { bytes: u64 },
    /// The file download is complete, or was skipped because the file is
    /// already on disk.
    Finished,
}

/// Options common to all download operations.
pub struct DownloadOptions<'a> {
    pub keep_tar: bool,
    pub no_resume: bool,
    /// Check the SHA-1 of the download against the MWA ASVO's. A resumed
    /// download, and a complete keep-tar file that is already on disk, are
    /// always checked, even when this is `false`.
    pub hash: bool,
    pub download_dir: &'a str,
    /// Called with each [`DownloadProgress`] event. `None` reports no
    /// progress. The library has no user interface of its own.
    pub progress: Option<&'a dyn Fn(DownloadProgress)>,
    pub download_number: usize,
    pub download_count: usize,
    /// How much data, in bytes, is held in memory before it is written to
    /// disk. See [`DEFAULT_DOWNLOAD_BUFFER_SIZE`](crate::DEFAULT_DOWNLOAD_BUFFER_SIZE).
    pub buffer_size: usize,
    /// How long to retry transient download failures before giving up.
    /// Zero disables retrying. See
    /// [`DEFAULT_DOWNLOAD_RETRY_DURATION`](crate::DEFAULT_DOWNLOAD_RETRY_DURATION).
    pub retry_duration: std::time::Duration,
    /// Asked between chunks of a download, and during the wait before a
    /// retry, whether to stop. When it returns `true`, the download stops
    /// with [`AsvoError::Interrupted`](crate::AsvoError::Interrupted), which
    /// is never retried. `None` never stops. The caller decides what a stop
    /// means (for example, Ctrl-C in the Python module); the library reads
    /// no signals. A file that is being written stays on disk, partial, so
    /// a later download can resume it.
    pub should_stop: Option<&'a dyn Fn() -> bool>,
}

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

/// The end of the name of a stream-untar resume file (see
/// [`SidecarWriter`]). The whole name is `.<tar file name>` and this.
const SIDECAR_SUFFIX: &str = ".giant-squid-resume.json";

/// The format version of the resume file. A file with another version is
/// not used.
const SIDECAR_VERSION: u32 = 1;

/// The shortest time between two writes of the resume file while a download
/// runs. A failed attempt always writes it.
const SIDECAR_WRITE_INTERVAL: Duration = Duration::from_secs(10);

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
        .retain(|j| j.obs_id == obs_id && j.job_state == JobState::Completed);

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
    if job.job_state != JobState::Completed {
        return Err(AsvoError::NotReady {
            job_id: job.job_id,
            job_state: job.job_state,
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
        match f.type_ {
            FileType::Acacia => {
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
                    retry_state.untar_checkpoint =
                        match checkpoint_from_sidecar(&out_path, f, &log_prefix, opts) {
                            Some(cp) => Some(cp),
                            None => untar_checkpoint_from_disk(
                                http_client,
                                url,
                                f,
                                job.job_id,
                                &log_prefix,
                                opts,
                            )?,
                        };
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
                    bytesize::ByteSize(
                        (f.size_bytes() * 1000)
                            .checked_div(elapsed_ms)
                            .unwrap_or_default(),
                    )
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
                    bytesize::ByteSize(f.size_bytes()).display().iec(),
                    duration_str,
                    throughput_str
                );
            }
            FileType::Dug => {
                error!(
                    "{} Files for Job are not reachable from the current host. \
                     You will find your job's files on the DUG filesystem.",
                    log_prefix
                );
            }
            FileType::Scratch => {
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
    file_info: &JobFile,
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
        job.job_type.map(|t| t.name()).unwrap_or_default(),
        bytesize::ByteSize(file_info.size_bytes()).display().iec(),
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

    report_started(
        opts,
        job.job_id,
        log_prefix,
        file_info.size_bytes(),
        resume_from,
    );

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
        report_started(
            opts,
            job.job_id,
            log_prefix,
            file_info.size_bytes(),
            resume_from,
        );
    }

    // Set when only part of the file was fetched this time, which changes
    // how the hash has to be checked (see below).
    let resumed = resume_from > 0;
    let stream_hasher = RefCell::new(Sha1::new());
    let mut reader = HashingReader {
        inner: response_reader(http_response),
        hasher: &stream_hasher,
        position: resume_from,
    };

    copy_with_progress(&mut reader, &mut out_file, opts.buffer_size, opts)?;

    // Drain any remaining bytes so the stream's hash covers everything.
    let final_bytes = io::copy(&mut reader, &mut io::sink())?;
    debug!("{} Read final bytes: {}", log_prefix, final_bytes);
    drop(reader);

    report(opts, DownloadProgress::Finished);

    // A resumed file joins bytes from more than one attempt (or run), so its
    // hash is checked even when `opts.hash` is not set.
    if !opts.hash && resumed {
        info!(
            "{} The download is resumed, so the hash is checked.",
            log_prefix
        );
    }
    if opts.hash || resumed {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);

        if resumed {
            // The stream's hash only covers the bytes fetched this time, so
            // it describes the tail rather than the file. Read the assembled
            // file back instead - slower, but only on a resumed download.
            check_file_sha1_hash(out_path, mwa_asvo_hash, job.job_id)?;
        } else {
            let hash = to_hex(&stream_hasher.into_inner().finalize());
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

/// Where a tar entry is unpacked.
enum EntryTarget {
    /// A file, at this path.
    File(PathBuf),
    /// A directory, at this path.
    Dir(PathBuf),
    /// Not unpacked: the entry's path is absolute, has a `..` part, or names
    /// no file, so it would not be written inside the download directory.
    Skip,
}

impl EntryTarget {
    /// The file that the entry is written to, or `None`.
    fn file_path(&self) -> Option<PathBuf> {
        match self {
            Self::File(path) => Some(path.clone()),
            Self::Dir(_) | Self::Skip => None,
        }
    }
}

/// Decide where the tar entry at `entry_path` is unpacked in `unpack_path`.
/// An entry whose path ends in `/` is a directory.
fn entry_target(unpack_path: &Path, entry_path: &Path) -> EntryTarget {
    let is_dir = entry_path.as_os_str().as_encoded_bytes().ends_with(b"/");
    let mut has_name = false;
    for component in entry_path.components() {
        match component {
            Component::Normal(_) => has_name = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return EntryTarget::Skip;
            }
        }
    }
    if is_dir {
        EntryTarget::Dir(unpack_path.join(entry_path))
    } else if has_name {
        EntryTarget::File(unpack_path.join(entry_path))
    } else {
        EntryTarget::Skip
    }
}

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
    /// The files that were finished before this member, for the resume file.
    /// Only the first `done_count` stamps belong to this checkpoint.
    done: DoneFiles,
    /// How many stamps of `done` belong to this checkpoint.
    done_count: usize,
}

/// The files that the attempts at one archive finished, in archive order.
/// Shared by the checkpoints, so that a checkpoint does not copy the list.
type DoneFiles = Rc<RefCell<DoneList>>;

/// See [`DoneFiles`].
#[derive(Default)]
struct DoneList {
    stamps: Vec<FileStamp>,
    /// A finished file could not be stamped (for example, its path is not
    /// UTF-8). A resume file would then not guard all the files that its
    /// hash state covers, so none is written.
    untracked: bool,
}

/// A finished file, as the resume file records it: if the file still has
/// this size and modification time, a rerun trusts that its bytes are the
/// ones that the saved hash state covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FileStamp {
    /// The tar entry path, relative to the download directory.
    path: String,
    size: u64,
    /// The modification time: whole seconds and nanoseconds after the Unix
    /// epoch.
    mtime_secs: u64,
    mtime_nanos: u32,
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
    /// The finished files, as for [`UntarCheckpoint::done`].
    done: DoneFiles,
    /// How many stamps of `done` are files before `start`.
    done_count: usize,
}

impl ResumePoint {
    /// An attempt that fetches the whole archive.
    fn from_start() -> Self {
        Self {
            start: 0,
            hasher: Sha1::new(),
            member: None,
            done: DoneFiles::default(),
            done_count: 0,
        }
    }
}

/// The part of a member that a failed attempt did not write.
struct MemberTail {
    /// The member's file, open for append, or `None` for a member that is
    /// not written to a file.
    file: Option<File>,
    /// The path of `file`, to stamp it when it is finished.
    path: Option<PathBuf>,
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
    file_info: &JobFile,
    job_id: AsvoJobId,
    out_path: &Path,
    log_prefix: &str,
    opts: &DownloadOptions,
    mwa_asvo_hash: &str,
    checkpoint: &mut Option<UntarCheckpoint>,
) -> Result<(), AsvoError> {
    let unpack_path = Path::new(opts.download_dir);

    let mut resume = match checkpoint.as_ref() {
        // A resumed download always checks the hash (see below), so the
        // bytes already on disk are always read back into the hasher.
        Some(cp) => resume_point(cp, true, log_prefix)?,
        None => ResumePoint::from_start(),
    };

    if resume.start > 0 {
        info!(
            "{} Resuming the download and untar to {} at byte {} of {}",
            log_prefix,
            unpack_path.display(),
            resume.start,
            file_info.size_bytes()
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

    report_started(
        opts,
        job_id,
        log_prefix,
        file_info.size_bytes(),
        resume.start,
    );

    // A resumed download joins bytes from more than one attempt (or run),
    // so its hash is checked even when `opts.hash` is not set.
    let check_hash = opts.hash || resume.start > 0;
    if !opts.hash && resume.start > 0 {
        info!(
            "{} The download is resumed, so the hash is checked.",
            log_prefix
        );
    }

    let sidecar = SidecarWriter::new(
        unpack_path,
        out_path,
        mwa_asvo_hash,
        file_info.size_bytes(),
        log_prefix,
    );
    let hasher = match untar_stream_with_sidecar(
        response_reader(response),
        resume,
        unpack_path,
        log_prefix,
        opts,
        checkpoint,
        Some(&sidecar),
    ) {
        Ok(hasher) => hasher,
        Err(e) => {
            // A later run can carry on from here, even if this run stops.
            if let Some(cp) = checkpoint.as_ref() {
                sidecar.write(cp);
            }
            return Err(e);
        }
    };

    report(opts, DownloadProgress::Finished);

    if check_hash {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        debug!("{} MWA ASVO hash: {}", log_prefix, mwa_asvo_hash);
        let hash = to_hex(&hasher.finalize());
        debug!("{} Our hash: {}", log_prefix, hash);
        if !hash.eq_ignore_ascii_case(mwa_asvo_hash) {
            // The bytes behind the checkpoint are now suspect, so the retry
            // must fetch the whole archive again.
            *checkpoint = None;
            sidecar.remove();
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
    sidecar.remove();
    Ok(())
}

/// Unpack the archive bytes from `source` into `unpack_path`, and return
/// the SHA1 of the whole archive.
///
/// `source` gives the archive from `resume.start` on. If `resume` is inside
/// a member, the rest of that member is written first. `checkpoint` is set
/// each time a member starts, so that it always tells a next attempt where
/// to carry on.
#[cfg(test)]
fn untar_stream(
    source: impl Read,
    resume: ResumePoint,
    unpack_path: &Path,
    log_prefix: &str,
    opts: &DownloadOptions,
    checkpoint: &mut Option<UntarCheckpoint>,
) -> Result<Sha1, AsvoError> {
    untar_stream_with_sidecar(
        source,
        resume,
        unpack_path,
        log_prefix,
        opts,
        checkpoint,
        None,
    )
}

/// [`untar_stream`], which also saves each checkpoint to `sidecar` (at most
/// every [`SIDECAR_WRITE_INTERVAL`]).
fn untar_stream_with_sidecar(
    source: impl Read,
    resume: ResumePoint,
    unpack_path: &Path,
    log_prefix: &str,
    opts: &DownloadOptions,
    checkpoint: &mut Option<UntarCheckpoint>,
    sidecar: Option<&SidecarWriter>,
) -> Result<Sha1, AsvoError> {
    let buffer_size = opts.buffer_size;
    // Stamps after the resume point belong to an attempt that failed later
    // in the archive: this attempt stamps those files again.
    let done = Rc::clone(&resume.done);
    done.borrow_mut().stamps.truncate(resume.done_count);
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
            Some(mut file) => {
                copy_with_progress(&mut data, &mut file, buffer_size, opts)?;
                drop(file);
                if let Some(path) = &tail.path {
                    stamp_done(&done, unpack_path, path, tail.size);
                }
            }
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
            let target = entry_target(unpack_path, &entry_path);

            let size = entry.size();
            let done_count = done.borrow().stamps.len();
            let new_checkpoint = checkpoint.insert(UntarCheckpoint {
                data_pos: base + entry.raw_file_position(),
                hasher: hasher.borrow().clone(),
                out_path: target.file_path(),
                size,
                done: Rc::clone(&done),
                done_count,
            });
            if let Some(sidecar) = sidecar {
                sidecar.write_if_due(new_checkpoint);
            }

            match target {
                EntryTarget::File(out_full) => {
                    debug!("{} Writing file {}", log_prefix, out_full.display());
                    let mut out_file = create_file_logged(&out_full, log_prefix)?;
                    copy_with_progress(
                        BufReader::with_capacity(buffer_size, entry),
                        &mut out_file,
                        buffer_size,
                        opts,
                    )?;
                    drop(out_file);
                    stamp_done(&done, unpack_path, &out_full, size);
                }
                EntryTarget::Dir(out_full) if !out_full.exists() => {
                    debug!("{} Creating directory {:?}", log_prefix, out_full);
                    std::fs::create_dir(&out_full).map_err(|e| {
                        error!(
                            "{} Error- cannot create directory {:?}",
                            log_prefix,
                            out_full.display()
                        );
                        AsvoError::IO(e)
                    })?;
                }
                EntryTarget::Dir(out_full) => {
                    debug!("{} Directory exists {}", log_prefix, out_full.display());
                }
                EntryTarget::Skip => {
                    // The tar parser passes over the entry's data, which the
                    // hash still covers.
                    warn!(
                        "{} Skipping tar entry {:?}: it would not be written inside the \
                         download directory.",
                        log_prefix, entry_path
                    );
                }
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
            path: cp.out_path.clone(),
            remaining: cp.size - on_disk,
            size: cp.size,
        }),
        done: Rc::clone(&cp.done),
        done_count: cp.done_count,
    })
}

/// Add the first `len` bytes of the file at `path` to `hasher`.
fn hash_file_prefix(path: &Path, len: u64, hasher: &mut Sha1) -> Result<(), AsvoError> {
    let copied = hash_reader(File::open(path)?.take(len), hasher)?;
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
/// retry then fetches the whole archive. Because files from an earlier run
/// are only trusted after this check, the hash is checked even when
/// [`DownloadOptions::hash`] is not set (see [`try_download_untar`]).
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
    file_info: &JobFile,
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
    file_info: &JobFile,
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
            archive_size: file_info.size_bytes(),
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
    let done = DoneFiles::default();
    {
        let mut tar = Archive::new(&mut reader);
        for entry in tar.entries()? {
            let entry = entry?;
            let entry_path = entry.path()?.to_path_buf();
            let target = entry_target(unpack_path, &entry_path);
            let size = entry.size();
            let data_pos = entry.raw_file_position();

            checkpoint = Some(UntarCheckpoint {
                data_pos,
                hasher: hasher.borrow().clone(),
                out_path: target.file_path(),
                size,
                done: Rc::clone(&done),
                done_count: done.borrow().stamps.len(),
            });

            // An entry that is skipped also stops the walk: its data is not
            // on disk, so the download carries on from it.
            let (out_full, is_dir) = match target {
                EntryTarget::Dir(out_full) if out_full.is_dir() => (out_full, true),
                EntryTarget::File(out_full)
                    if std::fs::metadata(&out_full)
                        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == size) =>
                {
                    (out_full, false)
                }
                _ => break,
            };

            debug!("{} Already on disk: {}", log_prefix, out_full.display());
            if files_on_disk == 0 {
                report_started(opts, job_id, log_prefix, file_info.size_bytes(), 0);
            }
            files_on_disk += 1;
            if !is_dir && size > 0 {
                // The member's data comes from its file.
                *local.borrow_mut() = Some(LocalData {
                    start: data_pos,
                    len: size,
                    file: File::open(&out_full)?,
                });
                copy_with_progress(entry, &mut io::sink(), opts.buffer_size, opts)?;
                bytes_on_disk += size;
            }
            if !is_dir {
                stamp_done(&done, unpack_path, &out_full, size);
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
    /// The member's file, read from its start.
    file: File,
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
        let result = match data.file.read(&mut buf[..want]) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "a file on disk became shorter while it was read",
            )),
            other => other,
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

// --- the resume file (sidecar) ---------------------------------------------

/// The resume file of a stream-untar download: `.<tar file name>` plus
/// [`SIDECAR_SUFFIX`], in the download directory.
fn sidecar_path(unpack_path: &Path, out_path: &Path) -> PathBuf {
    let tar_name = out_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    unpack_path.join(format!(".{tar_name}{SIDECAR_SUFFIX}"))
}

/// Stamp a file that is now finished, and add it to `done`. A file that
/// cannot be stamped marks `done` as untracked (see [`DoneList::untracked`]).
fn stamp_done(done: &DoneFiles, unpack_path: &Path, out_full: &Path, size: u64) {
    let stamp = file_stamp(unpack_path, out_full);
    let mut done = done.borrow_mut();
    match stamp {
        Some(stamp) if stamp.size == size => done.stamps.push(stamp),
        _ => done.untracked = true,
    }
}

/// The stamp of the file at `out_full` as it is now, or `None` if it cannot
/// be made.
fn file_stamp(unpack_path: &Path, out_full: &Path) -> Option<FileStamp> {
    let path = out_full
        .strip_prefix(unpack_path)
        .ok()?
        .to_str()?
        .to_string();
    let metadata = std::fs::metadata(out_full).ok()?;
    let mtime = metadata
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?;
    Some(FileStamp {
        path,
        size: metadata.len(),
        mtime_secs: mtime.as_secs(),
        mtime_nanos: mtime.subsec_nanos(),
    })
}

/// The contents of the resume file.
#[derive(Serialize, Deserialize)]
struct Sidecar {
    /// See [`SIDECAR_VERSION`].
    version: u32,
    /// The MWA ASVO SHA1 of the archive, which identifies the download.
    archive_sha1: String,
    archive_size: u64,
    /// The checkpoint (see [`UntarCheckpoint`]).
    data_pos: u64,
    member_size: u64,
    /// The checkpoint member's tar entry path, or `None` for a member that
    /// is not written to a file.
    member_path: Option<String>,
    /// The serialised SHA1 state at `data_pos`, in base64.
    hasher_state: String,
    /// The files finished before the checkpoint member.
    files: Vec<FileStamp>,
}

/// Saves the checkpoints of one stream-untar download to its resume file,
/// so that a later run can carry on without reading the finished files
/// again.
///
/// A later run trusts the saved hash state if every finished file still
/// has its stamped size and modification time (see
/// [`checkpoint_from_sidecar`]). The final hash check still runs.
struct SidecarWriter<'a> {
    path: PathBuf,
    unpack_path: &'a Path,
    archive_sha1: &'a str,
    archive_size: u64,
    log_prefix: &'a str,
    last_write: Cell<Instant>,
}

impl<'a> SidecarWriter<'a> {
    fn new(
        unpack_path: &'a Path,
        out_path: &Path,
        archive_sha1: &'a str,
        archive_size: u64,
        log_prefix: &'a str,
    ) -> Self {
        Self {
            path: sidecar_path(unpack_path, out_path),
            unpack_path,
            archive_sha1,
            archive_size,
            log_prefix,
            last_write: Cell::new(Instant::now()),
        }
    }

    /// Write `cp` if the last write was [`SIDECAR_WRITE_INTERVAL`] ago.
    fn write_if_due(&self, cp: &UntarCheckpoint) {
        if self.last_write.get().elapsed() >= SIDECAR_WRITE_INTERVAL {
            self.write(cp);
        }
    }

    /// Write `cp`. A failure is logged and does not stop the download: the
    /// resume file is only a short cut.
    fn write(&self, cp: &UntarCheckpoint) {
        self.last_write.set(Instant::now());
        match self.contents(cp) {
            None => debug!(
                "{} A finished file could not be stamped, so no resume file is written.",
                self.log_prefix
            ),
            Some(sidecar) => {
                if let Err(e) = self.write_atomically(&sidecar) {
                    warn!(
                        "{} Could not write the resume file {:?}: {}",
                        self.log_prefix, self.path, e
                    );
                }
            }
        }
    }

    /// The resume file for `cp`, or `None` if it would not guard every file
    /// that the hash state covers.
    fn contents(&self, cp: &UntarCheckpoint) -> Option<Sidecar> {
        let done = cp.done.borrow();
        if done.untracked {
            return None;
        }
        let member_path = match &cp.out_path {
            Some(path) => Some(
                path.strip_prefix(self.unpack_path)
                    .ok()?
                    .to_str()?
                    .to_string(),
            ),
            None => None,
        };
        Some(Sidecar {
            version: SIDECAR_VERSION,
            archive_sha1: self.archive_sha1.to_string(),
            archive_size: self.archive_size,
            data_pos: cp.data_pos,
            member_size: cp.size,
            member_path,
            hasher_state: BASE64.encode(cp.hasher.serialize().as_slice()),
            files: done.stamps.get(..cp.done_count)?.to_vec(),
        })
    }

    /// Write to a temporary file, then rename it, so that a reader never
    /// sees a half-written resume file.
    fn write_atomically(&self, sidecar: &Sidecar) -> io::Result<()> {
        let mut tmp_name = self.path.clone().into_os_string();
        tmp_name.push(".tmp");
        let tmp_path = PathBuf::from(tmp_name);
        std::fs::write(&tmp_path, serde_json::to_vec(sidecar)?)?;
        std::fs::rename(&tmp_path, &self.path)
    }

    /// Delete the resume file: the download finished, or its bytes are
    /// suspect.
    fn remove(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => warn!(
                "{} Could not delete the resume file {:?}: {}",
                self.log_prefix, self.path, e
            ),
        }
    }
}

/// Read the resume file of an earlier run of this download, and return its
/// checkpoint, or `None` if there is no usable resume file.
///
/// A resume file is used only if it is for this archive (same SHA1 and
/// size), and every file it stamped still has its size and modification
/// time. The files are not read again: their bytes are in the saved hash
/// state. The hash is still checked at the end, as for every resumed
/// download (see [`try_download_untar`]). Without a usable resume file, the download
/// looks for files on disk instead (see [`untar_checkpoint_from_disk`]).
fn checkpoint_from_sidecar(
    out_path: &Path,
    file_info: &JobFile,
    log_prefix: &str,
    opts: &DownloadOptions,
) -> Option<UntarCheckpoint> {
    let unpack_path = Path::new(opts.download_dir);
    let path = sidecar_path(unpack_path, out_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(
                "{} Could not read the resume file {:?} ({}). Checking the files on disk instead.",
                log_prefix, path, e
            );
            return None;
        }
    };
    match parse_sidecar(&bytes, unpack_path, file_info) {
        Ok(checkpoint) => {
            info!(
                "{} Found a resume file from an earlier download: {} files are already on disk. \
                 Only the rest of the archive is fetched.",
                log_prefix, checkpoint.done_count
            );
            Some(checkpoint)
        }
        Err(reason) => {
            info!(
                "{} Not using the resume file {:?}: {}. Checking the files on disk instead.",
                log_prefix, path, reason
            );
            None
        }
    }
}

/// The checkpoint in a resume file, or why the file cannot be used.
fn parse_sidecar(
    bytes: &[u8],
    unpack_path: &Path,
    file_info: &JobFile,
) -> Result<UntarCheckpoint, String> {
    let sidecar: Sidecar =
        serde_json::from_slice(bytes).map_err(|e| format!("it cannot be read ({e})"))?;
    if sidecar.version != SIDECAR_VERSION {
        return Err("it is from another version of giant-squid".to_string());
    }
    let same_archive = file_info
        .sha1
        .as_deref()
        .is_some_and(|sha1| sha1.eq_ignore_ascii_case(&sidecar.archive_sha1))
        && sidecar.archive_size == file_info.size_bytes()
        && sidecar.data_pos <= file_info.size_bytes();
    if !same_archive {
        return Err("it is for another download".to_string());
    }

    for stamp in &sidecar.files {
        let EntryTarget::File(out_full) = entry_target(unpack_path, Path::new(&stamp.path)) else {
            return Err(format!("it names an unsafe path {:?}", stamp.path));
        };
        if file_stamp(unpack_path, &out_full).as_ref() != Some(stamp) {
            return Err(format!(
                "{:?} changed after the earlier download",
                stamp.path
            ));
        }
    }

    let out_path = match &sidecar.member_path {
        None => None,
        Some(member_path) => match entry_target(unpack_path, Path::new(member_path)) {
            EntryTarget::File(out_full) => Some(out_full),
            _ => return Err(format!("it names an unsafe path {member_path:?}")),
        },
    };

    let state_bytes = BASE64
        .decode(&sidecar.hasher_state)
        .map_err(|e| format!("its hash state cannot be read ({e})"))?;
    let state = SerializedState::<Sha1>::try_from(state_bytes.as_slice())
        .map_err(|_| "its hash state has the wrong length".to_string())?;
    let hasher =
        Sha1::deserialize(&state).map_err(|_| "its hash state cannot be read".to_string())?;

    let done_count = sidecar.files.len();
    Ok(UntarCheckpoint {
        data_pos: sidecar.data_pos,
        hasher,
        out_path,
        size: sidecar.member_size,
        done: Rc::new(RefCell::new(DoneList {
            stamps: sidecar.files,
            untracked: false,
        })),
        done_count,
    })
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

/// What the output file on disk means for the download about to happen.
///
/// This used to be signalled by returning an offset equal to the expected
/// file size, with a comment saying the caller should return early - but the
/// caller never checked, so an already-complete file was downloaded again.
/// An explicit outcome makes the "nothing to do" case impossible to miss.
enum OutputTarget {
    /// Nothing to fetch: the file is already complete and matches the MWA
    /// ASVO hash.
    AlreadyDone { reason: &'static str },

    /// Fetch into this file, starting `offset` bytes in. An `offset` of 0
    /// is a download from the beginning.
    Download { file: File, offset: u64 },
}

/// Prepare the output file for a keep-tar download, from the file that is
/// already at `out_path`, if there is one.
///
/// A complete file that matches the MWA ASVO hash is not fetched again. Any
/// other file is downloaded again from the start, except a partial file when
/// `no_resume` is not set: that download carries on from the end of the file.
/// A file that is larger than the download cannot be part of it, so it is
/// downloaded again too.
fn prepare_output_file(
    out_path: &PathBuf,
    no_resume: bool,
    file_info: &JobFile,
    mwa_asvo_hash: &str,
    job_id: AsvoJobId,
    log_prefix: &str,
) -> Result<OutputTarget, AsvoError> {
    let start_again = || -> Result<OutputTarget, AsvoError> {
        Ok(OutputTarget::Download {
            file: create_file_logged(out_path, log_prefix)?,
            offset: 0,
        })
    };

    if !out_path.try_exists()? {
        return start_again();
    }

    let file_size_bytes = std::fs::metadata(out_path)?.len();

    if file_size_bytes == file_info.size_bytes() {
        info!(
            "{} Checking downloaded file hash against provided MWA ASVO hash for {:?}...",
            log_prefix, out_path
        );
        if check_file_sha1_hash(out_path, mwa_asvo_hash, job_id).is_ok() {
            return Ok(OutputTarget::AlreadyDone {
                reason: "File exists, is the correct size and matches the MWA ASVO hash.",
            });
        }
        warn!(
            "{} File exists and is the correct size, but the hash does not match \
             the provided MWA ASVO hash. Restarting download...",
            log_prefix
        );
        return start_again();
    }

    if file_size_bytes > file_info.size_bytes() {
        warn!(
            "{} {:?} is larger than the file to download. Restarting download...",
            log_prefix, out_path
        );
        return start_again();
    }

    if no_resume {
        info!(
            "{} Partial file exists, and --no-resume was set. Restarting download...",
            log_prefix
        );
        return start_again();
    }

    // A partial file, and resuming is allowed: append to what's there.
    Ok(OutputTarget::Download {
        file: File::options().append(true).open(out_path)?,
        offset: file_size_bytes,
    })
}
