// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO API (version 2).
//!
//! This is intentionally separate from [`crate::mwa_asvo::AsvoError`], which
//! covers the file download path (job lookup, hash checks, HTTP transfer).
//! This enum covers talking to the API itself: authentication and the
//! structured error responses (`ErrorResponse`).

use thiserror::Error;

use super::openapi::error::ConversionError;
use super::openapi::{ErrorResponse, FieldError};

/// An error of a request to the MWA ASVO API, or of its arguments.
#[derive(Error, Debug)]
pub enum AsvoApiError {
    /// No API key was given: the config's API key is empty, or the
    /// environment variable `variable` is not defined (see
    /// [`client_config_from_env`](crate::client_config_from_env)).
    #[error("No MWA ASVO API key was given{hint}", hint = missing_key_hint(.variable))]
    MissingAuthKey {
        /// The environment variable that should hold the key, when the key
        /// was read from the environment.
        variable: Option<&'static str>,
    },

    /// Login or token refresh against the MWA ASVO API failed.
    ///
    /// When the server sent an `ErrorResponse`, it is `response`, as for
    /// [`AsvoApiError::ApiError`], and `message` is its message. Otherwise
    /// `message` is the server's body (or what went wrong with the tokens),
    /// and there is no `response`.
    #[error(
        "Authentication with MWA ASVO failed{code_suffix}: {message}{extra}",
        code_suffix = error_code_suffix(response.as_deref()),
        extra = api_error_extra(response.as_deref())
    )]
    AuthenticationFailed {
        message: String,
        /// The server's reply, if it was an `ErrorResponse`. (Boxed, so
        /// that the error stays small: see `clippy::result_large_err`.)
        response: Option<Box<ErrorResponse>>,
    },

    /// A value generated from the MWA ASVO OpenAPI schema failed to
    /// validate (e.g. didn't satisfy a schema constraint like a length or
    /// range limit).
    #[error("{0}")]
    Conversion(#[from] ConversionError),

    /// A job argument is outside what the MWA ASVO OpenAPI schema allows.
    /// Found before any request is sent; see [`super::validate`].
    #[error("Invalid {name}: {message}")]
    InvalidParameter {
        /// The OpenAPI field name of the argument.
        name: &'static str,
        /// What is wrong with it, for example `must be between 0.1 and 1
        /// (got 1.5)`.
        message: String,
    },

    /// Failed to deserialise JSON returned by the MWA ASVO.
    #[error("Couldn't decode JSON from the MWA ASVO response: {0}")]
    BadJson(#[from] serde_json::Error),

    /// An error from the reqwest crate.
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),

    /// The server responded with a non-success status code, in the
    /// structured `ErrorResponse` shape.
    ///
    /// The message is the error code and the server's message, then what
    /// the server gave of its `detail`, its `suggestion`, the field errors
    /// and the request ID, each on a line of its own. Nothing is rewritten:
    /// the text is the server's.
    #[error(
        "MWA ASVO returned an error ({code}): {message}{extra}",
        code = response.error_code,
        message = response.message,
        extra = api_error_extra(Some(response))
    )]
    ApiError {
        /// The HTTP status code of the reply.
        status: u16,
        /// The reply. An empty `field_errors` is `None`, as if the server
        /// had sent none. (Boxed, so that the error stays small: see
        /// `clippy::result_large_err`.)
        response: Box<ErrorResponse>,
    },

    /// The server responded with a non-success status code, but the body
    /// wasn't in the structured `ErrorResponse` shape we expected. The
    /// `status` and `message` are the same fields as those of
    /// [`AsvoError::HttpError`](crate::AsvoError::HttpError), which is for a
    /// failed file download (an error of the file server, not of the API;
    /// it also has the `job_id`).
    #[error("The server responded with status code {status}, message:\n{message}")]
    BadStatus {
        /// The HTTP status code.
        status: u16,
        /// The body of the reply.
        message: String,
    },
}

/// The part of an [`AsvoApiError::ApiError`] message after the error code
/// and message: the detail and the suggestion, one line for each field
/// error, then the request ID. A part that the server did not give is left
/// out.
fn api_error_extra(response: Option<&ErrorResponse>) -> String {
    let Some(response) = response else {
        return String::new();
    };
    let mut extra = String::new();
    if let Some(detail) = &response.detail {
        extra.push_str(&format!("\n  Detail: {detail}"));
    }
    if let Some(suggestion) = &response.suggestion {
        extra.push_str(&format!("\n  Suggestion: {suggestion}"));
    }
    for e in response.field_errors.iter().flatten() {
        let FieldError { field, message, .. } = e;
        extra.push_str(&format!("\n  {field}: {message}"));
    }
    if let Some(id) = &response.request_id {
        extra.push_str(&format!("\n  (request ID: {id})"));
    }
    extra
}

impl AsvoApiError {
    /// Whether the same request might succeed if it is sent again later: it
    /// did not reach the server or got no complete reply (a timeout, a
    /// refused or dropped connection), or the server said that it is
    /// overloaded or in error (HTTP 429 or 5xx). A refusal of the request
    /// itself (any other 4xx), a failed login, a bad argument and a reply
    /// that cannot be decoded are not.
    pub fn is_transient(&self) -> bool {
        match self {
            AsvoApiError::Reqwest(e) => {
                e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
            }
            AsvoApiError::ApiError { status, .. } | AsvoApiError::BadStatus { status, .. } => {
                reqwest::StatusCode::from_u16(*status).is_ok_and(|code| {
                    code.is_server_error() || code == reqwest::StatusCode::TOO_MANY_REQUESTS
                })
            }
            _ => false,
        }
    }

    /// An [`AsvoApiError::AuthenticationFailed`] with only a message: a
    /// failure that is not an error reply from the server (for example a
    /// token that cannot be decoded).
    pub(crate) fn authentication_failed(message: impl Into<String>) -> Self {
        AsvoApiError::AuthenticationFailed {
            message: message.into(),
            response: None,
        }
    }

    /// An [`AsvoApiError::ApiError`] for the reply `response` with HTTP
    /// status `status`.
    pub(crate) fn api_error(status: u16, response: ErrorResponse) -> Self {
        AsvoApiError::ApiError {
            status,
            response: Box::new(tidy(response)),
        }
    }

    /// An [`AsvoApiError::AuthenticationFailed`] for the reply `response`.
    pub(crate) fn authentication_failed_with(response: ErrorResponse) -> Self {
        AsvoApiError::AuthenticationFailed {
            message: response.message.clone(),
            response: Some(Box::new(tidy(response))),
        }
    }
}

/// `response` as an error variant holds it: an empty `field_errors` is
/// `None`, as if the server had sent none.
fn tidy(mut response: ErrorResponse) -> ErrorResponse {
    if response.field_errors.as_ref().is_some_and(Vec::is_empty) {
        response.field_errors = None;
    }
    response
}

/// The error code of an [`AsvoApiError::AuthenticationFailed`] message, as
/// ` (CODE)`, or nothing when the server gave no code.
fn error_code_suffix(response: Option<&ErrorResponse>) -> String {
    response
        .map(|r| {
            let code = &r.error_code;
            format!(" ({code})")
        })
        .unwrap_or_default()
}

/// The end of the message of [`AsvoApiError::MissingAuthKey`]: what to set,
/// when the key comes from the environment.
fn missing_key_hint(variable: &Option<&'static str>) -> String {
    match variable {
        Some(name) => format!(": set the {name} environment variable."),
        None => ".".to_string(),
    }
}
