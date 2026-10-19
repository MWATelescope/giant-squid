// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Downloading the files of MWA ASVO jobs: the plain (keep-tar) and the
//! stream-untar transfer, resume, retries, the hash check and the resume
//! file.

#[cfg(test)]
mod tests;

use crate::check_file_sha1_hash;
use crate::helpers::{check_sha1_hash, hash_reader};
use crate::obs_id::ObsId;

use super::api::openapi::Type as FileType;
use super::redact::{redact_url, redact_urls_in_text};
use super::retry::{sleep_unless_stopped, Backoff};
use super::{AsvoError, AsvoJob, AsvoJobId, AsvoJobVec, JobFile, JobState};

use std::cell::{Cell, RefCell};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use log::{debug, info, warn};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use serde::{Deserialize, Serialize};
use sha1::digest::common::hazmat::{SerializableState, SerializedState};
use sha1::{Digest, Sha1};
use tar::Archive;

mod no_follow;
use no_follow::{open_for_writing, refused_a_symlink, BelowError, DownloadRoot, WriteMode};

/// The number of bytes in one MiB.
pub const BYTES_PER_MIB: usize = 1024 * 1024;

/// The smallest [`DownloadOptions::buffer_size`], in bytes. A buffer of 0
/// bytes writes nothing, so a download would end with an empty file.
pub const MIN_DOWNLOAD_BUFFER_SIZE: usize = 1;

/// The default [`DownloadOptions::buffer_size`]: 100 MiB.
pub const DEFAULT_DOWNLOAD_BUFFER_SIZE: usize = 100 * BYTES_PER_MIB;

/// The default [`DownloadOptions::retry_duration`]: how long a download
/// keeps retrying transient failures before giving up.
pub const DEFAULT_DOWNLOAD_RETRY_DURATION: Duration = Duration::from_secs(900);

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
        /// The MWA ASVO Job ID.
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
    /// Keep the tar file that the MWA ASVO delivers. `false` unpacks it while
    /// it arrives (stream-untar).
    pub keep_tar: bool,
    /// Do not resume a partial file that was there before the download
    /// started: download it again from the start. A complete keep-tar file
    /// that matches the MWA ASVO hash is still skipped.
    pub no_resume: bool,
    /// Check the SHA-1 of the download against the MWA ASVO's. A resumed
    /// download, and a complete keep-tar file that is already on disk, are
    /// always checked, even when this is `false`.
    pub hash: bool,
    /// The directory to download to, and to move the files of a reachable
    /// Scratch job into. It must exist.
    pub download_dir: &'a str,
    /// Called with each [`DownloadProgress`] event. `None` reports no
    /// progress. The library has no user interface of its own.
    pub progress: Option<&'a dyn Fn(DownloadProgress)>,
    /// The number of this download in a series (from 1), for the log label
    /// `[download_number/download_count]`.
    pub download_number: usize,
    /// How many downloads the series has, for the log label.
    pub download_count: usize,
    /// How much data, in bytes, is held in memory before it is written to
    /// disk. At least [`MIN_DOWNLOAD_BUFFER_SIZE`](crate::MIN_DOWNLOAD_BUFFER_SIZE);
    /// a download with a smaller value fails with
    /// [`AsvoError::InvalidOption`](crate::AsvoError::InvalidOption). See
    /// [`DEFAULT_DOWNLOAD_BUFFER_SIZE`](crate::DEFAULT_DOWNLOAD_BUFFER_SIZE).
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

impl DownloadOptions<'_> {
    /// Check that every option has a value that a download can use.
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidOption`] for the first option that has not.
    pub(crate) fn check(&self) -> Result<(), AsvoError> {
        if self.buffer_size < MIN_DOWNLOAD_BUFFER_SIZE {
            return Err(AsvoError::InvalidOption {
                name: "buffer_size",
                message: format!(
                    "must be at least {MIN_DOWNLOAD_BUFFER_SIZE} (got {})",
                    self.buffer_size
                ),
            });
        }
        Ok(())
    }
}

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

/// One minute. A download that takes longer has its time logged in minutes
/// and seconds.
const MINUTE: Duration = Duration::from_secs(60);

/// The number of milliseconds in one second, for the throughput of a
/// download.
const MILLIS_PER_SECOND: u64 = 1000;

/// Look up a single job by Job ID from the supplied list and download it.
pub(crate) fn download_by_job_id(
    http_client: &Client,
    jobs: &AsvoJobVec,
    job_id: AsvoJobId,
    opts: &DownloadOptions,
) -> Result<AsvoJob, AsvoError> {
    opts.check()?;
    debug!("Attempting to download Job ID {job_id}");
    // A Job ID is unique. If the list ever has it twice, the first is used.
    let job = jobs
        .0
        .iter()
        .find(|j| j.job_id() == job_id)
        .ok_or(AsvoError::NoAsvoJob(job_id))?;
    download_job(http_client, job, opts)?;
    Ok(job.clone())
}

/// Look up a single ready job by Obs ID from the supplied list and download it.
/// Fails if zero, or more than one, ready jobs match the Obs ID.
pub(crate) fn download_by_obs_id(
    http_client: &Client,
    jobs: &AsvoJobVec,
    obs_id: ObsId,
    opts: &DownloadOptions,
) -> Result<AsvoJob, AsvoError> {
    opts.check()?;
    debug!("Attempting to download Obs ID {obs_id}");
    let mut of_obs_id = jobs.0.iter().filter(|j| j.obs_id() == obs_id).peekable();
    if of_obs_id.peek().is_none() {
        return Err(AsvoError::NoObsId(obs_id));
    }
    let ready: Vec<&AsvoJob> = of_obs_id
        .filter(|j| j.job_state == JobState::Completed)
        .collect();
    match ready.as_slice() {
        [] => Err(AsvoError::NoJobReadyForObsId(obs_id)),
        [job] => {
            download_job(http_client, job, opts)?;
            Ok((*job).clone())
        }
        _ => Err(AsvoError::TooManyObsIds(obs_id)),
    }
}

/// Download all files for a single job, dispatching by delivery type.
fn download_job(
    http_client: &Client,
    job: &AsvoJob,
    opts: &DownloadOptions,
) -> Result<(), AsvoError> {
    let job_id = job.job_id();
    if job.job_state != JobState::Completed {
        return Err(AsvoError::NotReady {
            job_id,
            job_state: job.job_state,
        });
    }

    let files = match &job.product {
        None => return Err(AsvoError::NoFiles(job_id)),
        Some(p) if p.files.is_empty() => return Err(AsvoError::NoFiles(job_id)),
        Some(p) => &p.files,
    };

    let obs_id = job.obs_id();
    let number = opts.download_number;
    let count = opts.download_count;
    let log_prefix = format!("Job ID {job_id} (Obs ID: {obs_id}) [{number}/{count}]:");

    for f in files {
        match f.type_ {
            FileType::Acacia => {
                let start_time = Instant::now();
                let no_url = || AsvoError::NoUrl { job_id };
                let url = f.url.as_deref().ok_or_else(no_url)?;
                // Every downloaded file is checked against the MWA ASVO hash
                // when it is resumed, so a file without one is refused before
                // any request.
                if f.sha1.is_none() {
                    return Err(AsvoError::NoHash { job_id });
                }

                debug!("{log_prefix} Downloading from url {}", redact_url(url));
                // The file is named after the last part of the URL's path.
                let file_name = reqwest::Url::parse(url)
                    .ok()
                    .and_then(|u| {
                        u.path_segments()
                            .and_then(|mut segments| segments.next_back().map(str::to_string))
                    })
                    .filter(|name| !name.is_empty())
                    .ok_or_else(no_url)?;
                let out_path = Path::new(opts.download_dir).join(file_name);

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
                                job_id,
                                &log_prefix,
                                opts,
                            )?,
                        };
                }

                // When the last attempt started, for the throughput.
                let attempt_start = Cell::new(start_time);
                #[allow(clippy::result_large_err)]
                let op = || {
                    attempt_start.set(Instant::now());
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
                    .map_err(|e| retry_class(e, job_id))
                };

                let fetched = retry_unless_stopped(Backoff::new(opts.retry_duration), op, opts)?;

                // Nothing fetched: the file was already on disk, and that
                // was logged.
                if fetched > 0 {
                    let text = completion_text(
                        f.size_bytes(),
                        fetched,
                        start_time.elapsed(),
                        attempt_start.get().elapsed(),
                    );
                    info!("{log_prefix} {text}");
                }
            }
            FileType::Dug => {
                return Err(AsvoError::FilesNotReachable {
                    job_id,
                    delivery: f.type_,
                    path: f.path.clone(),
                });
            }
            FileType::Scratch => {
                let no_path = || AsvoError::NoPath { job_id };
                let path = f.path.as_deref().ok_or_else(no_path)?;
                let path_obj = Path::new(path);
                let folder_name = path_obj.file_name().ok_or_else(no_path)?;
                let target = Path::new(opts.download_dir).join(folder_name);
                let target_display = target.display();

                // The target is checked first, so that nothing is replaced.
                match move_target(path_obj, &target, job_id)? {
                    MoveTarget::AlreadyThere => {
                        info!("{log_prefix} The files are already in the download directory: {target_display}.");
                    }
                    MoveTarget::AlreadyMoved => {
                        info!(
                            "{log_prefix} The files were moved to the download directory by an earlier \
                             download: {target_display}."
                        );
                    }
                    MoveTarget::Free if !path_obj.exists() => {
                        return Err(AsvoError::FilesNotReachable {
                            job_id,
                            delivery: f.type_,
                            path: f.path.clone(),
                        });
                    }
                    MoveTarget::Free => {
                        info!(
                            "{log_prefix} Files for Job are reachable from the current host. \
                             Moving {path} to {target_display}."
                        );
                        warn_about_links_pointing_outside(path_obj, &log_prefix);
                        move_path(path_obj, &target)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Sort the error of one download attempt into a failure that the retry
/// loop gives up on (permanent) or tries again (transient).
///
/// An IO error, or a file or directory that cannot be created, is permanent,
/// because it is usually a problem with the local disk, except when an IO
/// error came from reading the download itself (see [`NetworkReader`]): a
/// dropped connection is worth another attempt.
#[allow(clippy::result_large_err)]
fn retry_class(e: AsvoError, job_id: AsvoJobId) -> Retry<AsvoError> {
    match &e {
        AsvoError::IO(io_error) if is_network_read_error(io_error) => Retry::Transient(e),
        AsvoError::IO(_)
        | AsvoError::CannotCreate { .. }
        | AsvoError::Interrupted
        | AsvoError::SymlinkInDownloadDir { .. }
        | AsvoError::NoHash { .. } => Retry::Permanent(e),
        AsvoError::HttpError { status: 404, .. } => {
            Retry::Permanent(AsvoError::Http404Error { job_id })
        }
        AsvoError::HttpError {
            status: 401 | 403, ..
        } => Retry::Permanent(e),
        _ => Retry::Transient(e),
    }
}

/// How the retry loop treats the error of one download attempt.
#[derive(Debug)]
enum Retry<E> {
    /// Give up: another attempt would fail in the same way.
    Permanent(E),
    /// Try again after a wait (see [`Backoff`]).
    Transient(E),
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
/// reporting. Returns the bytes that this attempt fetched: 0 when the file
/// was already on disk.
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
) -> Result<u64, AsvoError> {
    let job_id = job.job_id();
    let mwa_asvo_hash = file_info
        .sha1
        .as_deref()
        .ok_or(AsvoError::NoHash { job_id })?;

    let job_type = job.job_type.map(|t| t.name()).unwrap_or_default();
    let size_text = bytesize::ByteSize(file_info.size_bytes()).display().iec();
    info!("{log_prefix} Download starting (type: {job_type}, {size_text})");

    if !opts.keep_tar {
        return try_download_untar(
            http_client,
            url,
            file_info,
            job_id,
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
        job_id,
        log_prefix,
    )? {
        OutputTarget::AlreadyDone { reason } => {
            info!("{log_prefix} {reason} Skipping {out_path:?}.");
            report(opts, DownloadProgress::Finished);
            return Ok(0);
        }
        OutputTarget::Download { file, offset } => (file, offset),
    };
    retry_state.wrote_tar = true;

    report_started(
        opts,
        job_id,
        log_prefix,
        file_info.size_bytes(),
        resume_from,
    );

    let action = if resume_from > 0 {
        "Resuming download of"
    } else {
        "Downloading"
    };
    info!("{log_prefix} {action} tar archive {out_path:?}");

    let http_response = send_checked(http_client, url, range_from(resume_from), job_id)?;

    // A server that ignores the range answers 200 with the whole file.
    // Appending that to a partial file would silently corrupt it, so
    // start again from the beginning instead.
    if range_was_ignored(&http_response, resume_from, log_prefix) {
        out_file = create_file(out_path)?;
        resume_from = 0;
        report_started(
            opts,
            job_id,
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
    debug!("{log_prefix} Read final bytes: {final_bytes}");
    let fetched = reader.position - resume_from;
    drop(reader);

    report(opts, DownloadProgress::Finished);

    if hash_is_checked(opts, resumed, log_prefix) {
        info!("{log_prefix} Checking downloaded file hash against provided MWA ASVO hash for {out_path:?}...");
        debug!("{log_prefix} MWA ASVO hash: {mwa_asvo_hash}");

        if resumed {
            // The stream's hash only covers the bytes fetched this time, so
            // it describes the tail rather than the file. Read the assembled
            // file back instead - slower, but only on a resumed download.
            check_file_sha1_hash(out_path, mwa_asvo_hash, job_id)?;
        } else {
            check_sha1_hash(stream_hasher.into_inner(), mwa_asvo_hash, job_id, out_path)?;
        }
        info!("{log_prefix} File matches the MWA ASVO provided hash.");
    }

    Ok(fetched)
}

// --- stream-untar ----------------------------------------------------------

/// Where a tar entry is unpacked.
enum EntryTarget {
    /// A file, at this path.
    File(PathBuf),
    /// A directory, at this path.
    Dir(PathBuf),
    /// Not unpacked: the entry's path is absolute, has a `..` part or names
    /// no file, so it might not be written inside the download directory.
    Skip,
    /// The entry's file, or a directory on the way to it, is a symbolic link
    /// that is already on disk (at this path). Writing through it would write
    /// wherever it points, so the download fails
    /// ([`AsvoError::SymlinkInDownloadDir`]).
    ThroughLink(PathBuf),
    /// Not unpacked: the entry is not a file or a directory (for example a
    /// symbolic link, a hard link or a device). giant-squid unpacks only
    /// files and directories.
    NotAFile,
}

impl EntryTarget {
    /// The file that the entry is written to, or `None`.
    fn file_path(&self) -> Option<PathBuf> {
        match self {
            Self::File(path) => Some(path.clone()),
            Self::Dir(_) | Self::Skip | Self::ThroughLink(_) | Self::NotAFile => None,
        }
    }
}

/// The first symbolic link that is already on disk on the way to a path below
/// `unpack_path`: `entry_path` itself, or a directory on the way to it.
/// Writing through such a link would write wherever the link points, which
/// can be outside the download directory. `unpack_path` itself may be a
/// link: only what is below it is checked.
fn symlink_on_the_way(unpack_path: &Path, entry_path: &Path) -> Option<PathBuf> {
    let mut path = unpack_path.to_path_buf();
    entry_path.components().find_map(|component| {
        path.push(component);
        std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
            .then(|| path.clone())
    })
}

/// Decide where the tar entry at `entry_path` is unpacked in `unpack_path`.
/// An entry whose path ends in `/` is a directory. An entry whose path is
/// absolute, has a `..` part or names no file is not unpacked
/// ([`EntryTarget::Skip`]), and one that goes through a symbolic link that is
/// already on disk is [`EntryTarget::ThroughLink`].
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
    if let Some(link) = symlink_on_the_way(unpack_path, entry_path) {
        EntryTarget::ThroughLink(link)
    } else if is_dir {
        EntryTarget::Dir(unpack_path.join(entry_path))
    } else if has_name {
        EntryTarget::File(unpack_path.join(entry_path))
    } else {
        EntryTarget::Skip
    }
}

/// Decide where the tar entry at `entry_path`, of the type `entry_type`, is
/// unpacked in `unpack_path` (see [`entry_target`]). A directory entry is a
/// directory, whether or not its path ends in `/`. Only files and
/// directories are unpacked: any other type is [`EntryTarget::NotAFile`].
fn tar_entry_target(
    unpack_path: &Path,
    entry_path: &Path,
    entry_type: tar::EntryType,
) -> EntryTarget {
    if entry_type.is_dir() {
        match entry_target(unpack_path, entry_path) {
            EntryTarget::File(path) => EntryTarget::Dir(path),
            other => other,
        }
    } else if entry_type.is_file() || entry_type.is_contiguous() {
        entry_target(unpack_path, entry_path)
    } else {
        EntryTarget::NotAFile
    }
}

/// Open the download directory, for making the files and directories of a tar
/// archive below it (see [`DownloadRoot`]). The directory must exist.
fn open_download_root(unpack_path: &Path) -> Result<DownloadRoot, AsvoError> {
    DownloadRoot::open(unpack_path).map_err(|source| {
        let path_display = unpack_path.display();
        AsvoError::IO(io::Error::new(
            source.kind(),
            format!("cannot open the download directory {path_display}: {source}"),
        ))
    })
}

/// The error of a file or directory below the download directory that could
/// not be made: a symbolic link on the way, or another failure.
fn below_error(error: BelowError) -> AsvoError {
    match error {
        BelowError::Symlink(path) => AsvoError::SymlinkInDownloadDir { path },
        BelowError::Io { path, source } => AsvoError::CannotCreate { path, source },
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
    /// The download directory that the member's file is below.
    unpack_path: PathBuf,
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
/// read back from disk. Returns the bytes that this attempt fetched.
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
) -> Result<u64, AsvoError> {
    let unpack_path = Path::new(opts.download_dir);

    let mut resume = match checkpoint.as_ref() {
        // A resumed download always checks the hash (see below), so the
        // bytes already on disk are read back into the hasher.
        Some(cp) => resume_point(cp, log_prefix)?,
        None => ResumePoint::from_start(),
    };

    let unpack_display = unpack_path.display();
    if resume.start > 0 {
        let start = resume.start;
        let archive_size = file_info.size_bytes();
        info!("{log_prefix} Resuming the download and untar to {unpack_display} at byte {start} of {archive_size}");
    } else {
        info!("{log_prefix} Downloading and untarring to {unpack_display}");
    }

    let response = send_checked(http_client, url, range_from(resume.start), job_id)?;

    // A server that ignores the range answers 200 with the whole archive.
    // Start again from the beginning: the members are written again, from
    // the start of the archive.
    if range_was_ignored(&response, resume.start, log_prefix) {
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

    let check_hash = hash_is_checked(opts, resume.start > 0, log_prefix);

    // The bytes that this attempt fetches, for the log of the download.
    let fetched = Cell::new(0);
    let sidecar = SidecarWriter::new(
        unpack_path,
        out_path,
        mwa_asvo_hash,
        file_info.size_bytes(),
        log_prefix,
    );
    let hasher = match untar_stream_with_sidecar(
        CountingReader {
            inner: response_reader(response),
            count: &fetched,
        },
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
        info!("{log_prefix} Checking downloaded file hash against provided MWA ASVO hash for {out_path:?}...");
        debug!("{log_prefix} MWA ASVO hash: {mwa_asvo_hash}");
        if let Err(e) = check_sha1_hash(hasher, mwa_asvo_hash, job_id, out_path) {
            // The bytes behind the checkpoint are now suspect, so the retry
            // must fetch the whole archive again.
            *checkpoint = None;
            sidecar.remove();
            return Err(e);
        }
        info!("{log_prefix} File matches the MWA ASVO provided hash.");
    }

    *checkpoint = None;
    sidecar.remove();
    Ok(fetched.get())
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
    let root = open_download_root(unpack_path)?;
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
            let entry_type = entry.header().entry_type();
            let target = tar_entry_target(unpack_path, &entry_path, entry_type);

            let size = entry.size();
            let done_count = done.borrow().stamps.len();
            let new_checkpoint = checkpoint.insert(UntarCheckpoint {
                unpack_path: unpack_path.to_path_buf(),
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
                    let out_display = out_full.display();
                    debug!("{log_prefix} Writing file {out_display}");
                    let mut out_file = root
                        .open_file(&entry_path, WriteMode::CreateOrTruncate)
                        .map_err(below_error)?;
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
                    debug!("{log_prefix} Creating directory {out_full:?}");
                    root.create_dir_all(&entry_path).map_err(below_error)?;
                }
                EntryTarget::Dir(out_full) => {
                    let out_display = out_full.display();
                    debug!("{log_prefix} Directory exists {out_display}");
                }
                EntryTarget::Skip => {
                    // The tar parser passes over the entry's data, which the
                    // hash still covers.
                    warn!("{log_prefix} Skipping tar entry {entry_path:?}: it would not be written inside the \
                         download directory (its path is absolute, has '..' or names no file).");
                }
                EntryTarget::ThroughLink(path) => {
                    return Err(AsvoError::SymlinkInDownloadDir { path });
                }
                EntryTarget::NotAFile => {
                    // As for `Skip`, the hash still covers the entry.
                    warn!("{log_prefix} Skipping tar entry {entry_path:?}: it is a {entry_type:?} entry. \
                         giant-squid unpacks only files and directories.");
                }
            }
        }
    }

    // Read the rest of the archive (its end-of-archive blocks), so that the
    // hash covers all of it.
    let final_bytes = io::copy(&mut reader, &mut io::sink())?;
    debug!("{log_prefix} Read final bytes: {final_bytes}");
    drop(reader);

    Ok(hasher.into_inner())
}

/// Find where the attempt after `checkpoint` starts: after the bytes of the
/// checkpoint's member that are already on disk.
///
/// Those bytes are read back from disk into the hasher, so that the SHA1
/// still covers the whole archive. A file that is
/// larger than its member cannot be this download's own, so the attempt
/// then starts again from the beginning of the archive.
fn resume_point(cp: &UntarCheckpoint, log_prefix: &str) -> Result<ResumePoint, AsvoError> {
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
                    "{log_prefix} {path:?} is larger than the tar member it is written from. \
                     Starting again from the beginning."
                );
                return Ok(ResumePoint::from_start());
            }
            if on_disk > 0 {
                hash_file_prefix(path, on_disk, &mut hasher)?;
            }
            // A resume file can name a member whose file was not created yet.
            // The path was made from `unpack_path`, so it is below it.
            let unpack_path = &cp.unpack_path;
            let relative = path.strip_prefix(unpack_path).map_err(|_| {
                AsvoError::IO(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{path:?} is not below the download directory"),
                ))
            })?;
            let file = open_download_root(unpack_path)?
                .open_file(relative, WriteMode::CreateOrAppend)
                .map_err(below_error)?;
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
        let path_display = path.display();
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("{path_display} became shorter while it was read"),
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

/// A reader that adds the number of bytes it reads to `count`.
struct CountingReader<'c, R> {
    inner: R,
    count: &'c Cell<u64>,
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get() + n as u64);
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
                "{log_prefix} Could not check for files from an earlier download ({e}). \
                 Downloading the whole archive."
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
            job_id,
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
            let target = tar_entry_target(unpack_path, &entry_path, entry.header().entry_type());
            let size = entry.size();
            let data_pos = entry.raw_file_position();

            checkpoint = Some(UntarCheckpoint {
                unpack_path: unpack_path.to_path_buf(),
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

            let out_display = out_full.display();
            debug!("{log_prefix} Already on disk: {out_display}");
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
        let size_text = bytesize::ByteSize(bytes_on_disk).display().iec();
        let unpack_display = unpack_path.display();
        info!("{log_prefix} Found {files_on_disk} files ({size_text}) from an earlier download in {unpack_display}. \
             Only the rest of the archive is fetched.");
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
    /// The job whose archive this is, for the errors of the requests.
    job_id: AsvoJobId,
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
        let from = self.position;
        let last = end - 1;
        let mut headers = HeaderMap::new();
        headers.insert(
            RANGE,
            HeaderValue::from_str(&format!("bytes={from}-{last}"))
                .expect("a byte range is always a valid header value"),
        );
        let response = send_checked(self.http_client, self.url, Some(headers), self.job_id)
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
            let wanted = end - self.position;
            let got = bytes.len();
            return Err(io::Error::other(format!(
                "asked for {wanted} bytes from byte {from}, but the server sent {got}"
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
        let log_prefix = &self.log_prefix;
        match self.contents(cp) {
            None => debug!(
                "{log_prefix} A finished file could not be stamped, so no resume file is written."
            ),
            Some(sidecar) => {
                if let Err(e) = self.write_atomically(&sidecar) {
                    let path = &self.path;
                    warn!("{log_prefix} Could not write the resume file {path:?}: {e}");
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
        // A link at the temporary path is refused, not written through.
        let mut tmp_file = open_for_writing(&tmp_path, WriteMode::CreateOrTruncate)?;
        tmp_file.write_all(&serde_json::to_vec(sidecar)?)?;
        drop(tmp_file);
        std::fs::rename(&tmp_path, &self.path)
    }

    /// Delete the resume file: the download finished, or its bytes are
    /// suspect.
    fn remove(&self) {
        let log_prefix = &self.log_prefix;
        let path = &self.path;
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => warn!("{log_prefix} Could not delete the resume file {path:?}: {e}"),
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
            warn!("{log_prefix} Could not read the resume file {path:?} ({e}). Checking the files on disk instead.");
            return None;
        }
    };
    match parse_sidecar(&bytes, unpack_path, file_info) {
        Ok(checkpoint) => {
            let done_count = checkpoint.done_count;
            info!("{log_prefix} Found a resume file from an earlier download: {done_count} files are already on disk. \
                 Only the rest of the archive is fetched.");
            Some(checkpoint)
        }
        Err(reason) => {
            info!("{log_prefix} Not using the resume file {path:?}: {reason}. Checking the files on disk instead.");
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
            let path = &stamp.path;
            return Err(format!("it names an unsafe path {path:?}"));
        };
        if file_stamp(unpack_path, &out_full).as_ref() != Some(stamp) {
            let path = &stamp.path;
            return Err(format!("{path:?} changed after the earlier download"));
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
        unpack_path: unpack_path.to_path_buf(),
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
                let remaining = self.remaining.unwrap_or_default();
                Err(network_error(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("The connection closed with {remaining} bytes of the download still to come"),
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

/// Whether the caller has asked the download to stop.
fn stop_requested(opts: &DownloadOptions) -> bool {
    opts.should_stop.is_some_and(|should_stop| should_stop())
}

/// Run `op`, and retry it under `backoff` while it fails with a transient
/// error. The wait before each retry is cut into steps of at most
/// `STOP_CHECK_INTERVAL` (see [`sleep_unless_stopped`]), and the retries end with
/// [`AsvoError::Interrupted`] when the caller asks the download to stop.
/// Otherwise a stop could wait for the whole back-off interval, which grows
/// to a minute.
fn retry_unless_stopped<T>(
    mut backoff: Backoff,
    mut op: impl FnMut() -> Result<T, Retry<AsvoError>>,
    opts: &DownloadOptions,
) -> Result<T, AsvoError> {
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(Retry::Permanent(err)) => return Err(err),
            Err(Retry::Transient(err)) => {
                let Some(wait) = backoff.next_backoff() else {
                    return Err(err);
                };
                debug!("Retrying the download in {wait:?}: {err}");
                sleep_unless_stopped(wait, opts.should_stop)?;
            }
        }
    }
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

/// Send an HTTP GET for a file of job `job_id` and check for a successful
/// status code. A failure is only returned: the caller reports it.
fn send_checked(
    http_client: &Client,
    url: &str,
    headers: Option<HeaderMap>,
    job_id: AsvoJobId,
) -> Result<reqwest::blocking::Response, AsvoError> {
    let mut req = http_client.get(url);
    if let Some(h) = headers {
        req = req.headers(h);
    }
    // The URL of a file is signed, and a request error shows the URL that
    // failed, so the error is made without it.
    let response = req.send().map_err(reqwest::Error::without_url)?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(AsvoError::HttpError {
            job_id,
            status: status.as_u16(),
            message: redact_urls_in_text(&body),
        });
    }
    Ok(response)
}

/// Whether the server ignored the range request of a download that resumes
/// from byte `from` (it answered with the whole file, not 206), logging a
/// warning if so. The caller then starts again from the beginning.
fn range_was_ignored(response: &reqwest::blocking::Response, from: u64, log_prefix: &str) -> bool {
    let ignored = from > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT;
    if ignored {
        warn!(
            "{log_prefix} Asked to resume from byte {from}, but the server sent the whole file. \
             Starting again from the beginning."
        );
    }
    ignored
}

/// Whether the hash of a download is checked: when [`DownloadOptions::hash`]
/// is set, and always for a `resumed` download, which joins bytes from more
/// than one attempt (or run). Logs when only the resume makes the check.
fn hash_is_checked(opts: &DownloadOptions, resumed: bool, log_prefix: &str) -> bool {
    if !opts.hash && resumed {
        info!("{log_prefix} The download is resumed, so the hash is checked.");
    }
    opts.hash || resumed
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

/// The log text of a finished download: the size of the file and the
/// time of the whole download. `fetched` is what the last attempt fetched
/// (a resumed download, or a retry, fetches less than the size), and the
/// throughput is of those bytes, in the time of that attempt (`attempt`).
fn completion_text(size: u64, fetched: u64, elapsed: Duration, attempt: Duration) -> String {
    let attempt_ms = u64::try_from(attempt.as_millis()).unwrap_or(u64::MAX);
    let throughput = match fetched
        .saturating_mul(MILLIS_PER_SECOND)
        .checked_div(attempt_ms)
    {
        Some(bytes_per_second) => {
            let rate = bytesize::ByteSize(bytes_per_second).display().iec();
            format!("{rate}/s")
        }
        None => "N/A".to_string(),
    };
    let duration = if elapsed > MINUTE {
        let minutes = elapsed.as_secs() / MINUTE.as_secs();
        let seconds = elapsed.as_secs_f64() % MINUTE.as_secs_f64();
        format!("{minutes} min {seconds:.2} s")
    } else {
        let seconds = elapsed.as_secs_f64();
        format!("{seconds:.3} s")
    };
    let size_text = bytesize::ByteSize(size).display().iec();
    if fetched == size {
        format!("Completed download of {size_text} in {duration} ({throughput})")
    } else {
        let fetched_text = bytesize::ByteSize(fetched).display().iec();
        format!("Completed download of {size_text} in {duration}. The last attempt fetched {fetched_text} ({throughput}).")
    }
}

/// What is at the path that the files of a Scratch job are moved to.
#[derive(Debug, PartialEq, Eq)]
enum MoveTarget {
    /// Nothing: the move can go there.
    Free,
    /// The files themselves: the download directory is where they are.
    AlreadyThere,
    /// A directory, and nothing at the Scratch path any more: an earlier
    /// download moved the job's files here.
    AlreadyMoved,
}

/// Check `to`, where the Scratch files at `from` would be moved, before the
/// move. Nothing that is there is replaced. A directory at `to` while there is
/// nothing at `from` is taken as the result of an earlier download of the same
/// job (the name is the Job ID), so a second run is not an error.
///
/// # Errors
///
/// [`AsvoError::SymlinkInDownloadDir`] if `to` is a symbolic link (even one
/// that points nowhere), and [`AsvoError::DestinationExists`] if something
/// else is there that is not `from` itself.
fn move_target(from: &Path, to: &Path, job_id: AsvoJobId) -> Result<MoveTarget, AsvoError> {
    match fs::symlink_metadata(to) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(MoveTarget::Free),
        Err(e) => Err(e.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AsvoError::SymlinkInDownloadDir {
            path: to.to_path_buf(),
        }),
        Ok(_) if same_path(from, to) => Ok(MoveTarget::AlreadyThere),
        Ok(metadata) if metadata.is_dir() && !path_exists(from) => Ok(MoveTarget::AlreadyMoved),
        Ok(_) => Err(AsvoError::DestinationExists {
            job_id,
            path: to.to_path_buf(),
        }),
    }
}

/// Whether anything is at `path`, also a symbolic link that points nowhere.
fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether `a` and `b` are the same file or directory.
fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Log a warning for each symbolic link in the Scratch job folder `root` that
/// points outside the folder. The move keeps a link as it is, so after it the
/// link points to the same place, which may not be there for the user.
fn warn_about_links_pointing_outside(root: &Path, log_prefix: &str) {
    for (link, target) in links_pointing_outside(root) {
        let link_display = link.display();
        let target_display = target.display();
        warn!(
            "{log_prefix} The link {link_display} points to {target_display}, outside the job folder. \
             It is moved as a link."
        );
    }
}

/// The symbolic links in the folder `root`, and below it, that point outside
/// it, with the place that each points to as the link has it. A link is not
/// followed. A folder that cannot be read is left out: this only warns.
fn links_pointing_outside(root: &Path) -> Vec<(PathBuf, PathBuf)> {
    let Ok(root) = fs::canonicalize(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    collect_links_pointing_outside(&root, &root, &mut found);
    found
}

/// The recursive part of [`links_pointing_outside`]: the links in `dir`, for
/// the folder `root`.
fn collect_links_pointing_outside(root: &Path, dir: &Path, found: &mut Vec<(PathBuf, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            let Ok(target) = fs::read_link(&path) else {
                continue;
            };
            // A relative target is relative to the folder of the link. If
            // the place is not there, the lexical form tells where it would
            // be.
            let joined = dir.join(&target);
            let resolved = fs::canonicalize(&joined).unwrap_or_else(|_| lexically_normal(&joined));
            if !resolved.starts_with(root) {
                found.push((path, target));
            }
        } else if file_type.is_dir() {
            collect_links_pointing_outside(root, &path, found);
        }
    }
}

/// `path` without its `.` parts, and with each `..` part taking away the part
/// before it. It does not look at the file system.
fn lexically_normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// Move the file or directory `from` to `to`. A rename does it when both are
/// on the same file system. Across file systems (for example from Pawsey's
/// /scratch to a home directory), `from` is copied to `to` and then removed.
/// If the copy fails, `from` is kept, and what was copied stays at `to`.
fn move_path(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            let from_display = from.display();
            let to_display = to.display();
            debug!("{from_display} is on another file system than {to_display}; copying it");
            copy_then_remove(from, to)
        }
        result => result,
    }
}

/// The move of [`move_path`] across file systems: copy `from` to `to`, then
/// remove `from`.
fn copy_then_remove(from: &Path, to: &Path) -> io::Result<()> {
    copy_recursively(from, to)?;
    if fs::symlink_metadata(from)?.is_dir() {
        fs::remove_dir_all(from)
    } else {
        fs::remove_file(from)
    }
}

/// Copy the file, symbolic link or directory `from` to `to`. A directory is
/// copied with everything in it. A symbolic link is copied as a link, not
/// followed. `to` must not exist yet.
fn copy_recursively(from: &Path, to: &Path) -> io::Result<()> {
    let file_type = fs::symlink_metadata(from)?.file_type();
    if file_type.is_symlink() {
        copy_symlink(from, to)
    } else if file_type.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_recursively(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Copy the symbolic link `from` to `to`, as a link to the same target.
#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(from)?, to)
}

/// Copy the symbolic link `from` to `to`. Off Unix (no Scratch file system),
/// the file it points to is copied.
#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to).map(|_| ())
}

/// Create a file. A failure is only returned, with the path: the caller
/// reports it.
///
/// A symbolic link at `path` is refused when the file is opened, so that it
/// cannot be put there between a check and the open
/// ([`AsvoError::SymlinkInDownloadDir`]).
fn create_file(path: &Path) -> Result<File, AsvoError> {
    open_for_writing(path, WriteMode::CreateOrTruncate).map_err(|source| {
        if refused_a_symlink(&source, path) {
            AsvoError::SymlinkInDownloadDir {
                path: path.to_path_buf(),
            }
        } else {
            AsvoError::CannotCreate {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

/// What the output file on disk means for the download about to happen:
/// nothing to do, or a download from an offset. The "nothing to do" case is
/// a variant of its own, so that a caller cannot miss it.
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
            file: create_file(out_path)?,
            offset: 0,
        })
    };

    // A symbolic link at `out_path` (even one that points nowhere) would make
    // the download write wherever it points, so it is refused.
    if std::fs::symlink_metadata(out_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(AsvoError::SymlinkInDownloadDir {
            path: out_path.clone(),
        });
    }

    if !out_path.try_exists()? {
        return start_again();
    }

    let file_size_bytes = std::fs::metadata(out_path)?.len();

    if file_size_bytes == file_info.size_bytes() {
        info!("{log_prefix} Checking downloaded file hash against provided MWA ASVO hash for {out_path:?}...");
        if check_file_sha1_hash(out_path, mwa_asvo_hash, job_id).is_ok() {
            return Ok(OutputTarget::AlreadyDone {
                reason: "File exists, is the correct size and matches the MWA ASVO hash.",
            });
        }
        warn!(
            "{log_prefix} File exists and is the correct size, but the hash does not match \
             the provided MWA ASVO hash. Restarting download..."
        );
        return start_again();
    }

    if file_size_bytes > file_info.size_bytes() {
        warn!(
            "{log_prefix} {out_path:?} is larger than the file to download. Restarting download..."
        );
        return start_again();
    }

    if no_resume {
        info!("{log_prefix} Partial file exists, and resuming is off (no_resume). Restarting download...");
        return start_again();
    }

    // A partial file, and resuming is allowed: append to what's there.
    Ok(OutputTarget::Download {
        file: open_for_writing(out_path, WriteMode::Append).map_err(|source| {
            if refused_a_symlink(&source, out_path) {
                AsvoError::SymlinkInDownloadDir {
                    path: out_path.clone(),
                }
            } else {
                AsvoError::IO(source)
            }
        })?,
        offset: file_size_bytes,
    })
}
