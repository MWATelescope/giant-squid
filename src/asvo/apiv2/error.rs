// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO v2 API.
//!
//! This is intentionally separate from [`crate::asvo::AsvoError`], which
//! covers the file download path (job lookup, hash checks, HTTP transfer).
//! This enum covers talking to the API itself: authentication and the
//! structured error responses (`ErrorResponse`, `HttpValidationError`).

use thiserror::Error;

use super::openapi::error::ConversionError;
use super::openapi::FieldError;

#[derive(Error, Debug)]
pub enum AsvoApiError {
    /// No API key was given: the config's API key is empty, or (in the
    /// CLI) the user's MWA_ASVO_API_KEY environment variable is not defined.
    #[error("MWA_ASVO_API_KEY is not defined.")]
    MissingAuthKey,

    /// Login or token refresh against the MWA ASVO v2 API failed.
    #[error("Authentication with MWA ASVO failed: {message}")]
    AuthenticationFailed { message: String },

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
    /// structured `ErrorResponse` shape. The fields are copied out of
    /// `super::openapi::ErrorResponse` individually rather than wrapping it
    /// directly, since thiserror needs a plain `Display` to format on.
    ///
    /// The message has the field errors and the request ID, if the server
    /// gave them, after the error code and message.
    #[error(
        "MWA ASVO returned an error ({error_code}): {message}{}",
        api_error_extra(field_errors, request_id.as_deref())
    )]
    ApiError {
        error_code: String,
        message: String,
        detail: Option<String>,
        suggestion: Option<String>,
        /// The fields that failed validation, and why. Empty if the server
        /// gave none.
        field_errors: Vec<FieldError>,
        /// The server's ID for the request, for a support request.
        request_id: Option<String>,
    },

    /// The server responded with a non-success status code, but the body
    /// wasn't in the structured `ErrorResponse` shape we expected.
    #[error("The server responded with status code {code}, message:\n{message}")]
    BadStatus {
        code: reqwest::StatusCode,
        message: String,
    },
}

/// The part of an [`AsvoApiError::ApiError`] message after the error code
/// and message: one line for each field error, then the request ID.
fn api_error_extra(field_errors: &[FieldError], request_id: Option<&str>) -> String {
    let mut extra = String::new();
    for e in field_errors {
        extra.push_str(&format!("\n  {}: {}", e.field, e.message));
    }
    if let Some(id) = request_id {
        extra.push_str(&format!("\n  (request ID: {id})"));
    }
    extra
}
