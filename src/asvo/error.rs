// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO.
use thiserror::Error;

use super::{AsvoJobID, AsvoJobState};
use crate::obsid::Obsid;

#[derive(Error, Debug)]
pub enum AsvoError {
    /// Tried to download a job that doesn't exist.
    #[error("MWA ASVO job ID {0} wasn't found in your list of jobs.")]
    NoAsvoJob(AsvoJobID),

    /// Tried to download an obsid that doesn't exist.
    #[error("Obsid {0} wasn't found in your list of jobs.")]
    NoObsid(Obsid),

    /// Tried to download an obsid where >1 jobs exist but none are ready.
    #[error("No job for Obsid {0} is ready for download.")]
    NoJobReadyForObsid(Obsid),

    /// Tried to download an obsid, but it's associated with multiple jobs.
    #[error(
        "Obsid {0} is associated with multiple ready jobs; cannot continue due to ambiguity. Try specifying the JobID instead of the ObsId in this case."
    )]
    TooManyObsids(Obsid),

    /// Tried to download a job that wasn't ready.
    #[error("MWA ASVO job ID {jobid} isn't ready; current status: {state}")]
    NotReady {
        jobid: AsvoJobID,
        state: AsvoJobState,
    },

    /// Tried to download a job with an empty file product array.
    #[error(
        "MWA ASVO job ID {0} doesn't have any files associated with it! This shouldn't happen."
    )]
    NoFiles(AsvoJobID),

    /// ASVO SHA1 hash for a file didn't match our hash.
    #[error("Hash mismatch for MWA ASVO job ID {jobid} file {file}:\n expected   {expected_hash}\n calculated {calculated_hash}")]
    HashMismatch {
        jobid: AsvoJobID,
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

    /// An error from the reqwest crate.
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),

    /// A parse error.
    #[error("{0}")]
    Parse(#[from] std::num::ParseIntError),

    /// An IO error.
    #[error("{0}")]
    IO(#[from] std::io::Error),

    // Error determining url for Acacia job
    #[error("Could not determine url for job {job_id:?}")]
    NoUrl { job_id: u32 },

    // Error determining path for Astro job
    #[error("Could not determine path for job {job_id:?}")]
    NoPath { job_id: u32 },

    // HTTP error code when downloading
    #[error("HTTP error {status} downloading file: {message}")]
    HttpError { status: u16, message: String },

    // HTTP 404 error code when downloading
    #[error("The file for job {job_id:?} you are trying to download no longer exists. It may have expired or been removed. Please contact support if think this is in error")]
    Http404Error { job_id: u32 },
}
