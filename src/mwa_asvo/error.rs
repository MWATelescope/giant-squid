// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO.

use thiserror::Error;

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
    #[error("MWA ASVO Job ID {job_id} (Obs ID: {obs_id}) has an error{}: {error}", error_code_suffix(.error_code))]
    JobFailed {
        job_id: AsvoJobId,
        obs_id: ObsId,
        error: String,
        error_code: Option<i64>,
    },

    /// A job from the API cannot be used: its ID is not a Job ID, or its
    /// `job_params` have no valid Obs ID.
    #[error("MWA ASVO job {id} cannot be used: {problem}")]
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
    #[error("Could not determine the URL for job {job_id}")]
    NoUrl { job_id: AsvoJobId },

    /// A file of a Scratch job has no path.
    #[error("Could not determine the path for job {job_id}")]
    NoPath { job_id: AsvoJobId },

    /// The file to download to is a symbolic link. The download would write
    /// wherever the link points, so it is refused.
    #[error("{path:?} is a symbolic link; giant-squid does not download through one. Remove it, or use another download directory.")]
    SymlinkInDownloadDir { path: std::path::PathBuf },

    /// The server answered a file download with an HTTP error.
    #[error("HTTP error {status} downloading file: {message}")]
    HttpError { status: u16, message: String },

    /// The file to download is not on the server (HTTP 404).
    #[error("The file for job {job_id} you are trying to download no longer exists. It may have expired or been removed. Please contact support if you think this is in error.")]
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

/// A request error of the download path is an API error of the kind
/// [`AsvoApiError::Reqwest`], as for every other request, so that there is
/// one kind for it.
impl From<reqwest::Error> for AsvoError {
    fn from(e: reqwest::Error) -> Self {
        AsvoError::AsvoApi(AsvoApiError::Reqwest(e))
    }
}
