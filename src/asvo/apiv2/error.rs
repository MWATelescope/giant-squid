// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Errors when interfacing with the MWA ASVO v2 API.
//!
//! This is intentionally separate from [`crate::asvo::AsvoError`], which is
//! full of variants specific to the v1 client (job states, the old delivery
//! options, etc). This enum currently only covers what `AsvoClientv2`'s
//! authentication needs; expect it to grow variants for the v2 API's
//! structured error responses (`ErrorResponse`, `HttpValidationError`) once
//! we implement request-sending for the imaging job endpoint.

use thiserror::Error;

use super::openapi::error::ConversionError;

#[derive(Error, Debug)]
pub enum Apiv2Error {
    /// User's MWA_ASVO_API_KEY environment variable is not defined.
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
    #[error("MWA ASVO returned an error ({error_code}): {message}")]
    ApiError {
        error_code: String,
        message: String,
        detail: Option<String>,
        suggestion: Option<String>,
    },

    /// The server responded with a non-success status code, but the body
    /// wasn't in the structured `ErrorResponse` shape we expected.
    #[error("The server responded with status code {code}, message:\n{message}")]
    BadStatus {
        code: reqwest::StatusCode,
        message: String,
    },
}
