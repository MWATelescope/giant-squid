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

use super::apiv2::openapi::{ErrorResponse, FieldError};
use super::{AsvoApiError, AsvoError, AsvoJobId};
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

/// A command-line argument cannot be used: a bad job ID or obsid list, a
/// download directory that does not exist, a bad option.
pub const ERROR_CODE_INVALID_ARGUMENT: &str = "INVALID_ARGUMENT";

/// An environment variable has a value that cannot be used.
pub const ERROR_CODE_INVALID_ENVIRONMENT: &str = "INVALID_ENVIRONMENT";

/// The job is not in the user's list of jobs. (The API's code.)
pub const ERROR_CODE_JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";

/// No job of the obsid is in the user's list of jobs.
pub const ERROR_CODE_OBS_ID_NOT_FOUND: &str = "OBS_ID_NOT_FOUND";

/// The job, or every job of the obsid, is not ready.
pub const ERROR_CODE_JOB_NOT_READY: &str = "JOB_NOT_READY";

/// The obsid has more than one ready job, so a job ID is needed.
pub const ERROR_CODE_AMBIGUOUS_OBS_ID: &str = "AMBIGUOUS_OBS_ID";

/// The job is in the error state. The `detail` is the job's own error code,
/// when the server gave one.
pub const ERROR_CODE_JOB_FAILED: &str = "JOB_FAILED";

/// The job has been cancelled.
pub const ERROR_CODE_JOB_CANCELLED: &str = "JOB_CANCELLED";

/// A job from the API cannot be used (a bad job ID or obsid in it).
pub const ERROR_CODE_INVALID_JOB: &str = "INVALID_JOB";

/// The job has no files.
pub const ERROR_CODE_NO_FILES: &str = "NO_FILES";

/// The job has no URL to download its files from.
pub const ERROR_CODE_NO_URL: &str = "NO_URL";

/// The job has no path of its files.
pub const ERROR_CODE_NO_PATH: &str = "NO_PATH";

/// A downloaded file does not have the hash that the MWA ASVO gave.
pub const ERROR_CODE_HASH_MISMATCH: &str = "HASH_MISMATCH";

/// The file to download to is a symbolic link.
pub const ERROR_CODE_SYMLINK_IN_DOWNLOAD_DIR: &str = "SYMLINK_IN_DOWNLOAD_DIR";

/// A file could not be read or written.
pub const ERROR_CODE_IO_ERROR: &str = "IO_ERROR";

/// The caller stopped the download.
pub const ERROR_CODE_INTERRUPTED: &str = "INTERRUPTED";

/// Any other error.
pub const ERROR_CODE_CLIENT_ERROR: &str = "CLIENT_ERROR";

/// An `ErrorResponse` with only the code `error_code` and the message
/// `message`. Also for an error that is not one of the library's, for
/// example a command-line argument that cannot be used.
pub fn new_error_response(error_code: &str, message: impl ToString) -> ErrorResponse {
    ErrorResponse {
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

impl AsvoApiError {
    /// The error as an `ErrorResponse`.
    ///
    /// An error that the MWA ASVO sent as an `ErrorResponse` is copied
    /// verbatim; this includes a failed login whose body is one. An HTTP
    /// error with another body is `HTTP_<status>`, with the status's reason
    /// as the message and the body, verbatim, as the detail.
    pub fn error_response(&self) -> ErrorResponse {
        match self {
            AsvoApiError::MissingAuthKey { .. } => {
                new_error_response(ERROR_CODE_MISSING_API_KEY, self)
            }
            AsvoApiError::AuthenticationFailed { message } => {
                serde_json::from_str::<ErrorResponse>(message)
                    .unwrap_or_else(|_| new_error_response(ERROR_CODE_AUTHENTICATION_FAILED, self))
            }
            AsvoApiError::Conversion(_) => new_error_response(ERROR_CODE_INVALID_PARAMETER, self),
            AsvoApiError::InvalidParameter { name, message } => ErrorResponse {
                field_errors: Some(vec![FieldError {
                    field: name.to_string(),
                    message: message.clone(),
                }]),
                ..new_error_response(ERROR_CODE_INVALID_PARAMETER, self)
            },
            AsvoApiError::BadJson(_) => new_error_response(ERROR_CODE_BAD_RESPONSE, self),
            AsvoApiError::Reqwest(_) => new_error_response(ERROR_CODE_NETWORK_ERROR, self),
            AsvoApiError::ApiError {
                error_code,
                message,
                detail,
                suggestion,
                field_errors,
                request_id,
            } => ErrorResponse {
                detail: detail.clone(),
                error_code: error_code.clone(),
                field_errors: (!field_errors.is_empty()).then(|| field_errors.clone()),
                message: message.clone(),
                request_id: request_id.clone(),
                suggestion: suggestion.clone(),
            },
            AsvoApiError::BadStatus { code, message } => ErrorResponse {
                detail: Some(message.clone()),
                ..new_error_response(
                    &http_error_code(code.as_u16()),
                    code.canonical_reason().unwrap_or(code.as_str()),
                )
            },
        }
    }
}

impl AsvoError {
    /// The error as an `ErrorResponse`. An API error is
    /// [`AsvoApiError::error_response`].
    pub fn error_response(&self) -> ErrorResponse {
        let code = match self {
            AsvoError::AsvoApi(e) => return e.error_response(),
            AsvoError::JobFailed { error_code, .. } => {
                return ErrorResponse {
                    detail: error_code.map(|c| c.to_string()),
                    ..new_error_response(ERROR_CODE_JOB_FAILED, self)
                }
            }
            AsvoError::HttpError { status, message } => {
                return ErrorResponse {
                    detail: Some(message.clone()),
                    ..new_error_response(&http_error_code(*status), self)
                }
            }
            AsvoError::Http404Error { .. } => {
                return new_error_response(
                    &http_error_code(reqwest::StatusCode::NOT_FOUND.as_u16()),
                    self,
                )
            }
            AsvoError::NoAsvoJob(_) => ERROR_CODE_JOB_NOT_FOUND,
            AsvoError::InvalidJob { .. } => ERROR_CODE_INVALID_JOB,
            AsvoError::JobCancelled(_) => ERROR_CODE_JOB_CANCELLED,
            AsvoError::NoObsId(_) => ERROR_CODE_OBS_ID_NOT_FOUND,
            AsvoError::NoJobReadyForObsId(_) | AsvoError::NotReady { .. } => {
                ERROR_CODE_JOB_NOT_READY
            }
            AsvoError::TooManyObsIds(_) => ERROR_CODE_AMBIGUOUS_OBS_ID,
            AsvoError::NoFiles(_) => ERROR_CODE_NO_FILES,
            AsvoError::HashMismatch { .. } => ERROR_CODE_HASH_MISMATCH,
            AsvoError::InvalidJobState { .. } | AsvoError::InvalidJobType { .. } => {
                ERROR_CODE_INVALID_ARGUMENT
            }
            AsvoError::InvalidEnvironment { .. } => ERROR_CODE_INVALID_ENVIRONMENT,
            AsvoError::IO(_) => ERROR_CODE_IO_ERROR,
            AsvoError::Interrupted => ERROR_CODE_INTERRUPTED,
            AsvoError::NoUrl { .. } => ERROR_CODE_NO_URL,
            AsvoError::NoPath { .. } => ERROR_CODE_NO_PATH,
            AsvoError::SymlinkInDownloadDir { .. } => ERROR_CODE_SYMLINK_IN_DOWNLOAD_DIR,
        };
        new_error_response(code, self)
    }

    /// The job that the error is about, if it is about one job.
    pub fn job_id(&self) -> Option<AsvoJobId> {
        match self {
            AsvoError::NoAsvoJob(job_id)
            | AsvoError::JobCancelled(job_id)
            | AsvoError::NoFiles(job_id)
            | AsvoError::JobFailed { job_id, .. }
            | AsvoError::NotReady { job_id, .. }
            | AsvoError::HashMismatch { job_id, .. }
            | AsvoError::NoUrl { job_id }
            | AsvoError::NoPath { job_id }
            | AsvoError::Http404Error { job_id } => Some(*job_id),
            _ => None,
        }
    }

    /// The obsid that the error is about, if it is about one obsid.
    pub fn obs_id(&self) -> Option<ObsId> {
        match self {
            AsvoError::NoObsId(obs_id)
            | AsvoError::NoJobReadyForObsId(obs_id)
            | AsvoError::TooManyObsIds(obs_id)
            | AsvoError::JobFailed { obs_id, .. } => Some(*obs_id),
            _ => None,
        }
    }
}

impl ParseError {
    /// The error as an `ErrorResponse`: an argument that cannot be used.
    pub fn error_response(&self) -> ErrorResponse {
        new_error_response(ERROR_CODE_INVALID_ARGUMENT, self)
    }
}

impl ObsIdError {
    /// The error as an `ErrorResponse`: an argument that cannot be used.
    pub fn error_response(&self) -> ErrorResponse {
        new_error_response(ERROR_CODE_INVALID_ARGUMENT, self)
    }
}

#[cfg(test)]
mod tests;
