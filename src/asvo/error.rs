// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO.
use thiserror::Error;

use super::{AsvoApiError, AsvoJobId, AsvoJobState};
use crate::obs_id::ObsId;

#[derive(Error, Debug)]
pub enum AsvoError {
    /// Tried to download (or check) a job that doesn't exist.
    #[error("MWA ASVO job ID {0} wasn't found in your list of jobs.")]
    NoAsvoJob(AsvoJobId),

    /// A checked job is in an error state. `error_code` is the server's
    /// code for the error, if it gave one; the message shows it as
    /// `(code N)`.
    #[error("MWA ASVO job ID {job_id} (obsid: {obs_id}) has an error{}: {error}", error_code_suffix(.error_code))]
    JobFailed {
        job_id: AsvoJobId,
        obs_id: ObsId,
        error: String,
        error_code: Option<i64>,
    },

    /// A checked job has expired.
    #[error("MWA ASVO job ID {0} has expired.")]
    JobExpired(AsvoJobId),

    /// A checked job has been cancelled.
    #[error("MWA ASVO job ID {0} has been cancelled.")]
    JobCancelled(AsvoJobId),

    /// Tried to download an obsid that doesn't exist.
    #[error("Obsid {0} wasn't found in your list of jobs.")]
    NoObsId(ObsId),

    /// Tried to download an obsid where >1 jobs exist but none are ready.
    #[error("No job for Obsid {0} is ready for download.")]
    NoJobReadyForObsId(ObsId),

    /// Tried to download an obsid, but it's associated with multiple jobs.
    #[error(
        "Obsid {0} is associated with multiple ready jobs; cannot continue due to ambiguity. Try specifying the JobID instead of the ObsId in this case."
    )]
    TooManyObsIds(ObsId),

    /// Tried to download a job that wasn't ready.
    #[error("MWA ASVO job ID {job_id} isn't ready; current status: {job_state}")]
    NotReady {
        job_id: AsvoJobId,
        job_state: AsvoJobState,
    },

    /// Tried to download a job with an empty file product array.
    #[error(
        "MWA ASVO job ID {0} doesn't have any files associated with it! This shouldn't happen."
    )]
    NoFiles(AsvoJobId),

    /// ASVO SHA1 hash for a file didn't match our hash.
    #[error("Hash mismatch for MWA ASVO job ID {job_id} file {file}:\n expected   {expected_hash}\n calculated {calculated_hash}")]
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

    /// An error from the reqwest crate.
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),

    /// An IO error.
    #[error("{0}")]
    IO(#[from] std::io::Error),

    /// [`DownloadOptions::should_stop`](super::DownloadOptions::should_stop)
    /// asked the download to stop.
    #[error("The download was stopped by the caller.")]
    Interrupted,

    // Error determining url for Acacia job
    #[error("Could not determine url for job {job_id:?}")]
    NoUrl { job_id: AsvoJobId },

    // Error determining path for Astro job
    #[error("Could not determine path for job {job_id:?}")]
    NoPath { job_id: AsvoJobId },

    // HTTP error code when downloading
    #[error("HTTP error {status} downloading file: {message}")]
    HttpError { status: u16, message: String },

    // HTTP 404 error code when downloading
    #[error("The file for job {job_id:?} you are trying to download no longer exists. It may have expired or been removed. Please contact support if think this is in error")]
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
