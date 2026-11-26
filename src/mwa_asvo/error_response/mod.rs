// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Every error of the library as the MWA ASVO's `ErrorResponse`, for output
//! that a program reads (`giant-squid --json`).
//!
//! An error that the MWA ASVO sent as an `ErrorResponse` is copied
//! verbatim. Every other error gets a giant-squid error code (the
//! `ERROR_CODE_*` constants), in the UPPER_SNAKE style of the API's codes,
//! and its message. Where the API has a code with the same meaning, that
//! code is used (for example [`ERROR_CODE_JOB_NOT_FOUND`]).
//!
//! The schema's `ErrorResponse` holds only the API's codes (`ErrorCode`),
//! so these errors are a [`ClientErrorResponse`]: the same JSON shape, with
//! the code as text.

use super::api::openapi::{ErrorResponse, FieldError};
use super::{MwaAsvoApiError, MwaAsvoError, MwaAsvoJobId};
use crate::obs_id::{ObsId, ObsIdError};
use crate::ParseError;

/// The prefix of the code of an HTTP error whose body is not an
/// `ErrorResponse`: `HTTP_` and the status code, for example `HTTP_503`.
pub const ERROR_CODE_HTTP_PREFIX: &str = "HTTP_";

/// No API key was given.
pub const ERROR_CODE_MISSING_API_KEY: &str = "MISSING_API_KEY";

/// Login or token refresh failed, and the server did not send an
/// `ErrorResponse`.
pub const ERROR_CODE_AUTHENTICATION_FAILED: &str = "AUTHENTICATION_FAILED";

/// A job argument is outside what the OpenAPI schema allows.
pub const ERROR_CODE_INVALID_PARAMETER: &str = "INVALID_PARAMETER";

/// The MWA ASVO sent a reply that could not be decoded.
pub const ERROR_CODE_BAD_RESPONSE: &str = "BAD_RESPONSE";

/// A request could not be sent, or no reply came (connection, timeout,
/// TLS).
pub const ERROR_CODE_NETWORK_ERROR: &str = "NETWORK_ERROR";

/// A command-line argument cannot be used: a bad Job ID or Obs ID list, a
/// download directory that does not exist, a bad option.
pub const ERROR_CODE_INVALID_ARGUMENT: &str = "INVALID_ARGUMENT";

/// An environment variable has a value that cannot be used.
pub const ERROR_CODE_INVALID_ENVIRONMENT: &str = "INVALID_ENVIRONMENT";

/// The job is not in the user's list of jobs. (The API's code.)
pub const ERROR_CODE_JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";

/// No job of the Obs ID is in the user's list of jobs.
pub const ERROR_CODE_OBS_ID_NOT_FOUND: &str = "OBS_ID_NOT_FOUND";

/// The job, or every job of the Obs ID, is not ready.
pub const ERROR_CODE_JOB_NOT_READY: &str = "JOB_NOT_READY";

/// The Obs ID has more than one ready job, so a Job ID is needed.
pub const ERROR_CODE_AMBIGUOUS_OBS_ID: &str = "AMBIGUOUS_OBS_ID";

/// The job is in the error state. The `detail` is the job's own error code,
/// when the server gave one.
pub const ERROR_CODE_JOB_FAILED: &str = "JOB_FAILED";

/// The job has been cancelled.
pub const ERROR_CODE_JOB_CANCELLED: &str = "JOB_CANCELLED";

/// A job from the API cannot be used (a bad Job ID or Obs ID in it).
pub const ERROR_CODE_INVALID_JOB: &str = "INVALID_JOB";

/// The job has no files.
pub const ERROR_CODE_NO_FILES: &str = "NO_FILES";

/// The download links of a job have expired: the job has to be resubmitted
/// ([`MwaAsvoError::JobExpired`]).
pub const ERROR_CODE_JOB_EXPIRED: &str = "JOB_EXPIRED";

/// The job has no URL to download its files from.
pub const ERROR_CODE_NO_URL: &str = "NO_URL";

/// A file of the job has no SHA-1 hash.
pub const ERROR_CODE_NO_HASH: &str = "NO_HASH";

/// The job's files are on a file system (DUG or Scratch) that this host
/// cannot reach.
pub const ERROR_CODE_FILES_NOT_REACHABLE: &str = "FILES_NOT_REACHABLE";

/// The job has no path of its files.
pub const ERROR_CODE_NO_PATH: &str = "NO_PATH";

/// A downloaded file does not have the hash that the MWA ASVO gave.
pub const ERROR_CODE_HASH_MISMATCH: &str = "HASH_MISMATCH";

/// The file to download to is a symbolic link.
pub const ERROR_CODE_SYMLINK_IN_DOWNLOAD_DIR: &str = "SYMLINK_IN_DOWNLOAD_DIR";

/// Something is already where the files of a Scratch job would be moved.
pub const ERROR_CODE_DESTINATION_EXISTS: &str = "DESTINATION_EXISTS";

/// A file could not be read or written.
pub const ERROR_CODE_IO_ERROR: &str = "IO_ERROR";

/// The caller stopped the download.
pub const ERROR_CODE_INTERRUPTED: &str = "INTERRUPTED";

/// Any other error.
pub const ERROR_CODE_CLIENT_ERROR: &str = "CLIENT_ERROR";

/// An error in the JSON shape of the MWA ASVO's `ErrorResponse`, but with
/// the error code as text, so that it can also hold a giant-squid code
/// (the `ERROR_CODE_*` constants). The fields are in the order in which
/// `ErrorResponse` writes them.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct ClientErrorResponse {
    /// Additional technical detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The machine-readable error code: the API's `ErrorCode` or a
    /// giant-squid code.
    pub error_code: String,
    /// Field-specific validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_errors: Option<Vec<FieldError>>,
    /// The user-friendly error message.
    pub message: String,
    /// The request ID for tracing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// The suggested resolution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl From<ErrorResponse> for ClientErrorResponse {
    /// The MWA ASVO's `ErrorResponse`, verbatim.
    fn from(response: ErrorResponse) -> Self {
        let ErrorResponse {
            detail,
            error_code,
            field_errors,
            message,
            request_id,
            suggestion,
        } = response;
        Self {
            detail,
            error_code: error_code.to_string(),
            field_errors,
            message,
            request_id,
            suggestion,
        }
    }
}

/// A [`ClientErrorResponse`] with only the code `error_code` and the message
/// `message`. Also for an error that is not one of the library's, for
/// example a command-line argument that cannot be used.
pub fn new_error_response(error_code: &str, message: impl ToString) -> ClientErrorResponse {
    ClientErrorResponse {
        detail: None,
        error_code: error_code.to_string(),
        field_errors: None,
        message: message.to_string(),
        request_id: None,
        suggestion: None,
    }
}

/// The code of an HTTP error whose body is not an `ErrorResponse`, for
/// example `HTTP_503`.
fn http_error_code(status: u16) -> String {
    format!("{ERROR_CODE_HTTP_PREFIX}{status}")
}

/// The reason phrase of the HTTP status `status`, for example `Bad
/// Gateway`, or the code itself if it has none.
fn status_reason(status: u16) -> String {
    reqwest::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
        .map_or_else(|| status.to_string(), str::to_string)
}

impl MwaAsvoApiError {
    /// The error in the shape of an `ErrorResponse`.
    ///
    /// An error that the MWA ASVO sent as an `ErrorResponse` is copied
    /// verbatim; this includes a failed login whose body is one. An HTTP
    /// error with another body is `HTTP_<status>`, with the status's reason
    /// as the message and the body, verbatim, as the detail.
    pub fn error_response(&self) -> ClientErrorResponse {
        match self {
            MwaAsvoApiError::MissingAuthKey { .. } => {
                new_error_response(ERROR_CODE_MISSING_API_KEY, self)
            }
            MwaAsvoApiError::AuthenticationFailed {
                response: Some(response),
                ..
            }
            | MwaAsvoApiError::Server { response, .. } => {
                ClientErrorResponse::from(ErrorResponse::clone(response))
            }
            MwaAsvoApiError::AuthenticationFailed { response: None, .. } => {
                new_error_response(ERROR_CODE_AUTHENTICATION_FAILED, self)
            }
            MwaAsvoApiError::Conversion(_) => {
                new_error_response(ERROR_CODE_INVALID_PARAMETER, self)
            }
            MwaAsvoApiError::InvalidParameter { name, message } => ClientErrorResponse {
                field_errors: Some(vec![FieldError {
                    field: name.to_string(),
                    message: message.clone(),
                }]),
                ..new_error_response(ERROR_CODE_INVALID_PARAMETER, self)
            },
            MwaAsvoApiError::BadJson(_) => new_error_response(ERROR_CODE_BAD_RESPONSE, self),
            MwaAsvoApiError::Http(_) => new_error_response(ERROR_CODE_NETWORK_ERROR, self),
            MwaAsvoApiError::BadStatus { status, message } => ClientErrorResponse {
                detail: Some(message.clone()),
                ..new_error_response(&http_error_code(*status), status_reason(*status))
            },
        }
    }
}

impl MwaAsvoError {
    /// The error in the shape of an `ErrorResponse`. An API error is
    /// [`MwaAsvoApiError::error_response`].
    pub fn error_response(&self) -> ClientErrorResponse {
        let code = match self {
            MwaAsvoError::Api(e) => return e.error_response(),
            MwaAsvoError::JobFailed { error_code, .. } => {
                return ClientErrorResponse {
                    detail: error_code.map(|c| c.to_string()),
                    ..new_error_response(ERROR_CODE_JOB_FAILED, self)
                }
            }
            MwaAsvoError::HttpStatus {
                status, message, ..
            } => {
                return ClientErrorResponse {
                    detail: Some(message.clone()),
                    ..new_error_response(&http_error_code(*status), self)
                }
            }
            MwaAsvoError::HttpNotFound { .. } => {
                return new_error_response(
                    &http_error_code(reqwest::StatusCode::NOT_FOUND.as_u16()),
                    self,
                )
            }
            MwaAsvoError::NoMwaAsvoJob(_) => ERROR_CODE_JOB_NOT_FOUND,
            MwaAsvoError::InvalidJob { .. } => ERROR_CODE_INVALID_JOB,
            MwaAsvoError::JobCancelled(_) => ERROR_CODE_JOB_CANCELLED,
            MwaAsvoError::NoObsId(_) => ERROR_CODE_OBS_ID_NOT_FOUND,
            MwaAsvoError::NoJobReadyForObsId(_) | MwaAsvoError::NotReady { .. } => {
                ERROR_CODE_JOB_NOT_READY
            }
            MwaAsvoError::TooManyObsIds(_) => ERROR_CODE_AMBIGUOUS_OBS_ID,
            MwaAsvoError::NoFiles(_) => ERROR_CODE_NO_FILES,
            MwaAsvoError::JobExpired { .. } => ERROR_CODE_JOB_EXPIRED,
            MwaAsvoError::HashMismatch { .. } => ERROR_CODE_HASH_MISMATCH,
            MwaAsvoError::InvalidJobState { .. }
            | MwaAsvoError::InvalidJobType { .. }
            | MwaAsvoError::InvalidOption { .. } => ERROR_CODE_INVALID_ARGUMENT,
            MwaAsvoError::InvalidEnvironment { .. } => ERROR_CODE_INVALID_ENVIRONMENT,
            MwaAsvoError::IO(_) | MwaAsvoError::CannotCreate { .. } => ERROR_CODE_IO_ERROR,
            MwaAsvoError::Interrupted => ERROR_CODE_INTERRUPTED,
            MwaAsvoError::NoUrl { .. } => ERROR_CODE_NO_URL,
            MwaAsvoError::NoPath { .. } => ERROR_CODE_NO_PATH,
            MwaAsvoError::NoHash { .. } => ERROR_CODE_NO_HASH,
            MwaAsvoError::FilesNotReachable { .. } => ERROR_CODE_FILES_NOT_REACHABLE,
            MwaAsvoError::SymlinkInDownloadDir { .. } => ERROR_CODE_SYMLINK_IN_DOWNLOAD_DIR,
            MwaAsvoError::DestinationExists { .. } => ERROR_CODE_DESTINATION_EXISTS,
        };
        new_error_response(code, self)
    }

    /// The job that the error is about, if it is about one job.
    pub fn job_id(&self) -> Option<MwaAsvoJobId> {
        match self {
            MwaAsvoError::NoMwaAsvoJob(job_id)
            | MwaAsvoError::JobCancelled(job_id)
            | MwaAsvoError::NoFiles(job_id)
            | MwaAsvoError::JobFailed { job_id, .. }
            | MwaAsvoError::NotReady { job_id, .. }
            | MwaAsvoError::JobExpired { job_id, .. }
            | MwaAsvoError::HashMismatch { job_id, .. }
            | MwaAsvoError::NoUrl { job_id }
            | MwaAsvoError::NoPath { job_id }
            | MwaAsvoError::NoHash { job_id }
            | MwaAsvoError::FilesNotReachable { job_id, .. }
            | MwaAsvoError::DestinationExists { job_id, .. }
            | MwaAsvoError::HttpStatus { job_id, .. }
            | MwaAsvoError::HttpNotFound { job_id } => Some(*job_id),
            _ => None,
        }
    }

    /// The Obs ID that the error is about, if it is about one Obs ID.
    pub fn obs_id(&self) -> Option<ObsId> {
        match self {
            MwaAsvoError::NoObsId(obs_id)
            | MwaAsvoError::NoJobReadyForObsId(obs_id)
            | MwaAsvoError::TooManyObsIds(obs_id)
            | MwaAsvoError::JobFailed { obs_id, .. } => Some(*obs_id),
            _ => None,
        }
    }
}

impl ParseError {
    /// The error in the shape of an `ErrorResponse`: an argument that cannot be
    /// used.
    pub fn error_response(&self) -> ClientErrorResponse {
        new_error_response(ERROR_CODE_INVALID_ARGUMENT, self)
    }
}

impl ObsIdError {
    /// The error in the shape of an `ErrorResponse`: an argument that cannot be
    /// used.
    pub fn error_response(&self) -> ClientErrorResponse {
        new_error_response(ERROR_CODE_INVALID_ARGUMENT, self)
    }
}

#[cfg(test)]
mod tests;
