// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO.

use thiserror::Error;

use super::api::openapi::Type as FileType;
use super::{AsvoApiError, AsvoJobId, JobState};
use crate::obs_id::ObsId;

/// An error of a download or a job check. An API request that fails during
/// one is [`AsvoError::AsvoApi`].
#[derive(Error, Debug)]
pub enum AsvoError {
    /// Tried to download (or check) a job that doesn't exist.
    #[error("MWA ASVO Job ID {0} wasn't found in your list of jobs.")]
    NoAsvoJob(AsvoJobId),

    /// A checked job is in an error state. `error_code` is the server's
    /// code for the error, if it gave one; the message shows it as
    /// `(code N)`.
    #[error("MWA ASVO Job ID {job_id} (Obs ID: {obs_id}) has an error{code_suffix}: {error}", code_suffix = error_code_suffix(.error_code))]
    JobFailed {
        job_id: AsvoJobId,
        obs_id: ObsId,
        error: String,
        error_code: Option<i64>,
    },

    /// A job from the API cannot be used: its ID is not a Job ID, or its
    /// `job_params` have no valid Obs ID.
    #[error("MWA ASVO Job ID {id} cannot be used: {problem}")]
    InvalidJob { id: i64, problem: String },

    /// A checked job has been cancelled.
    #[error("MWA ASVO Job ID {0} has been cancelled.")]
    JobCancelled(AsvoJobId),

    /// Tried to download an Obs ID that doesn't exist.
    #[error("Obs ID {0} wasn't found in your list of jobs.")]
    NoObsId(ObsId),

    /// Tried to download an Obs ID that has jobs, but none of them is ready.
    #[error("No job for Obs ID {0} is ready for download.")]
    NoJobReadyForObsId(ObsId),

    /// Tried to download an Obs ID, but it's associated with multiple jobs.
    #[error(
        "Obs ID {0} is associated with multiple ready jobs; cannot continue due to ambiguity. Try specifying the Job ID instead of the Obs ID in this case."
    )]
    TooManyObsIds(ObsId),

    /// Tried to download a job that wasn't ready.
    #[error("MWA ASVO Job ID {job_id} isn't ready; current status: {job_state}")]
    NotReady {
        job_id: AsvoJobId,
        job_state: JobState,
    },

    /// Tried to download a job that has no files.
    #[error("MWA ASVO Job ID {0} has no files.")]
    NoFiles(AsvoJobId),

    /// ASVO SHA1 hash for a file didn't match our hash.
    #[error("Hash mismatch for MWA ASVO Job ID {job_id} file {file}:\n expected   {expected_hash}\n calculated {calculated_hash}")]
    HashMismatch {
        job_id: AsvoJobId,
        file: String,
        calculated_hash: String,
        expected_hash: String,
    },

    /// Job state parsing error
    #[error("Could not parse job state from str: {str}")]
    InvalidJobState { str: String },

    /// Job type parsing error
    #[error("Could not parse job type from str: {str}")]
    InvalidJobType { str: String },

    /// An environment variable that the settings are read from has a value
    /// that cannot be used (see [`DownloadSettings::from_env`](super::DownloadSettings::from_env)).
    /// `problem` completes the sentence: `is not valid. (It should be ...)`.
    #[error("Environment variable {name}='{value}' {problem}")]
    InvalidEnvironment {
        name: String,
        value: String,
        problem: String,
    },

    /// A download option has a value that cannot be used, for example a
    /// `buffer_size` of 0. Found before any request is sent. (An environment
    /// variable with such a value is [`AsvoError::InvalidEnvironment`].)
    #[error("Invalid {name}: {message}")]
    InvalidOption {
        /// The name of the option, as in [`DownloadOptions`](super::DownloadOptions).
        name: &'static str,
        /// What is wrong with it, for example `must be at least 1 (got 0)`.
        message: String,
    },

    /// An MWA ASVO API call failed, for example the job list request that
    /// a download does first.
    #[error("{0}")]
    AsvoApi(#[from] AsvoApiError),

    /// An IO error.
    #[error("{0}")]
    IO(#[from] std::io::Error),

    /// [`DownloadOptions::should_stop`](super::DownloadOptions::should_stop)
    /// asked the download to stop.
    #[error("The download was stopped by the caller.")]
    Interrupted,

    /// A file of an Acacia job has no URL to download it from.
    #[error("Could not determine the URL for Job ID {job_id}")]
    NoUrl { job_id: AsvoJobId },

    /// A file of an Acacia job has no SHA-1 hash, so the download could not
    /// be checked. Nothing is downloaded.
    #[error("MWA ASVO Job ID {job_id} has a file with no SHA-1 hash. Please report this to asvo_support@mwatelescope.org.")]
    NoHash { job_id: AsvoJobId },

    /// The files of a DUG or Scratch job are not on a file system that this
    /// host can reach, so there is nothing to download here. `path` is
    /// where the MWA ASVO delivered them, if it said.
    #[error("{text}", text = files_not_reachable_message(*.job_id, *.delivery, .path.as_deref()))]
    FilesNotReachable {
        job_id: AsvoJobId,
        delivery: FileType,
        path: Option<String>,
    },

    /// A file of a Scratch job has no path.
    #[error("Could not determine the path for Job ID {job_id}")]
    NoPath { job_id: AsvoJobId },

    /// The file to download to is a symbolic link. The download would write
    /// wherever the link points, so it is refused.
    #[error("{path:?} is a symbolic link; giant-squid does not download through one. Remove it, or use another download directory.")]
    SymlinkInDownloadDir { path: std::path::PathBuf },

    /// The files of a Scratch job are moved into the download directory,
    /// but something is already at the path that they would be moved to.
    /// giant-squid does not replace it.
    #[error("MWA ASVO Job ID {job_id}: {path:?} already exists, and giant-squid does not replace it. Remove it, or use another download directory. (An earlier download may have moved the files of this job there.)")]
    DestinationExists {
        /// The job whose files were to be moved.
        job_id: AsvoJobId,
        /// What is already there.
        path: std::path::PathBuf,
    },

    /// The file server answered a file download with an HTTP error. This is
    /// not an error of the MWA ASVO API, so it is not an [`AsvoApiError`].
    /// `status` and `message` are the same fields as those of
    /// [`AsvoApiError::BadStatus`] (an API reply that is not an
    /// `ErrorResponse`); a download is always for one job, so this error also
    /// has its `job_id`.
    #[error("HTTP error {status} downloading a file of Job ID {job_id}: {message}")]
    HttpError {
        /// The job whose file was being downloaded.
        job_id: AsvoJobId,
        /// The HTTP status code.
        status: u16,
        /// The body of the reply.
        message: String,
    },

    /// A file or directory of a download could not be created.
    #[error("Cannot create {path}: {source}", path = .path.display())]
    CannotCreate {
        /// The file or directory.
        path: std::path::PathBuf,
        /// Why it could not be created.
        source: std::io::Error,
    },

    /// The signed download links of a job have expired. The MWA ASVO signs
    /// them for 7 days; the job has to be resubmitted to get new ones. Found
    /// from the `Expires` of the URLs before any request, or from the file
    /// server's `AccessDenied` answer (HTTP 403) during a download.
    #[error("{}", job_expired_message(*.job_id, .expired))]
    JobExpired {
        /// The job whose links expired.
        job_id: AsvoJobId,
        /// When they expired, if known (the `Expires` of the URL).
        expired: Option<jiff::Timestamp>,
    },

    /// The file to download is not on the server (HTTP 404).
    #[error("The file for Job ID {job_id} you are trying to download no longer exists. It may have expired or been removed. Please contact support if you think this is in error.")]
    Http404Error { job_id: AsvoJobId },
}

/// The text that follows "has an error" in the message of
/// [`AsvoError::JobFailed`]: ` (code N)`, or nothing if the server gave no
/// code.
fn error_code_suffix(error_code: &Option<i64>) -> String {
    match error_code {
        Some(code) => format!(" (code {code})"),
        None => String::new(),
    }
}

/// The message of [`AsvoError::FilesNotReachable`].
fn files_not_reachable_message(
    job_id: AsvoJobId,
    delivery: FileType,
    path: Option<&str>,
) -> String {
    let place = match delivery {
        FileType::Dug => "the DUG file system".to_string(),
        FileType::Scratch => "the Pawsey scratch file system".to_string(),
        FileType::Acacia => "Acacia".to_string(),
    };
    let at = path.map(|p| format!(" at {p}")).unwrap_or_default();
    format!("MWA ASVO Job ID {job_id}: the files are on {place}{at}, which this host cannot reach.")
}

/// A request error of the download path is an API error of the kind
/// [`AsvoApiError::Reqwest`], as for every other request, so that there is
/// one kind for it.
impl From<reqwest::Error> for AsvoError {
    fn from(e: reqwest::Error) -> Self {
        AsvoError::AsvoApi(AsvoApiError::Reqwest(e))
    }
}

/// The message of [`AsvoError::JobExpired`].
fn job_expired_message(job_id: AsvoJobId, expired: &Option<jiff::Timestamp>) -> String {
    let days = super::types::SIGNED_URL_LIFETIME_DAYS;
    let when = match expired {
        Some(time) => format!("expired on {time}"),
        None => "have expired".to_string(),
    };
    format!("The download links of Job ID {job_id} {when} (they last {days} days). Resubmit the job to get new ones.")
}
