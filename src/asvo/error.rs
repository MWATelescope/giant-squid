// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/*!
 * Errors when interfacing with the MWA ASVO.
*/

use reqwest::StatusCode;
use thiserror::Error;

use super::{AsvoJobID, AsvoJobState, AsvoJobType};
use crate::obsid::Obsid;

#[derive(Error, Debug)]
pub enum AsvoError {
    /// User's MWA_ASVO_API_KEY environment variable is not defined.
    #[error("MWA_ASVO_API_KEY is not defined.")]
    MissingAuthKey,

    /// The response had a status code other than 200.
    #[error("The server responded with status code {code}, message:\n{message}")]
    BadStatus { code: StatusCode, message: String },

    /// The response indicates a bad request.
    #[error("The server responded with status code {code}, message:\n{message}")]
    BadRequest { code: u32, message: String },

    /// Tried to download a job that doesn't exist.
    #[error("ASVO job ID {0} wasn't found in your list of jobs.")]
    NoAsvoJob(AsvoJobID),

    /// Tried to download an obsid that doesn't exist.
    #[error("Obsid {0} wasn't found in your list of jobs.")]
    NoObsid(Obsid),

    /// Report to the caller that this job has expired.
    #[error("ASVO job ID {0} has expired.")]
    Expired(AsvoJobID),

    /// Report to the caller that this job has been cancelled.
    #[error("ASVO job ID {0} has been cancelled.")]
    Cancelled(AsvoJobID),

    /// Tried to download an obsid, but it's associated with multiple jobs.
    #[error("Obsid {0} is associated with multiple jobs; cannot continue due to ambiguity.")]
    TooManyObsids(Obsid),

    /// Tried to download a job that wasn't ready.
    #[error("ASVO job ID {jobid} isn't ready; current status: {state}")]
    NotReady {
        jobid: AsvoJobID,
        state: AsvoJobState,
    },

    /// Tried to download a job with an empty file product array.
    #[error("ASVO job ID {0} doesn't have any files associated with it! This shouldn't happen.")]
    NoFiles(AsvoJobID),

    /// Tried to submit a job type that isn't supported.
    #[error("Tried to submit an ASVO job with a type ({0}) that isn't supported.")]
    UnsupportedType(AsvoJobType),

    /// ASVO SHA1 hash for a file didn't match our hash.
    #[error("Hash mismatch for ASVO job ID {jobid} file {file}:\n expected   {expected_hash}\n calculated {calculated_hash}")]
    HashMismatch {
        jobid: AsvoJobID,
        file: String,
        calculated_hash: String,
        expected_hash: String,
    },

    /// Tried to download a job that has an error against it.
    #[error("ASVO job ID {jobid} (obsid: {obsid}) has an error: {error}")]
    UpstreamError {
        jobid: AsvoJobID,
        obsid: Obsid,
        error: String,
    },

    /// Failed to deserialise the JSON from the body of the response from a
    /// "get_jobs" request.
    #[error("Couldn't decode the JSON from the ASVO response: {0}")]
    BadJson(#[from] serde_json::error::Error),

    /// An error from the reqwest crate.
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),

    /// A parse error.
    #[error("{0}")]
    Parse(#[from] std::num::ParseIntError),

    /// An IO error.
    #[error("{0}")]
    IO(#[from] std::io::Error),
}
