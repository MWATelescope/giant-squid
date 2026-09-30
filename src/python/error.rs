// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python exceptions `AsvoApiError` and `AsvoError`.
//!
//! Each exception has the name of the Rust error enum. Its `kind` attribute
//! is the name of the Rust variant (for example `"ApiError"`), and it has one
//! attribute for each of that variant's fields. The exception's message is
//! the Rust error's message.

use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use super::types::PyAsvoJobState;
// The library's error enums have the same names as the Python exceptions,
// so they are used through this path.
use crate::asvo as lib;

pyo3::create_exception!(
    mwa_giant_squid,
    AsvoApiError,
    PyException,
    "An MWA ASVO API call failed. `kind` is the Rust `AsvoApiError` variant."
);

pyo3::create_exception!(
    mwa_giant_squid,
    AsvoError,
    PyException,
    "A download or job check failed. `kind` is the Rust `AsvoError` variant."
);

/// The name of the attribute that holds the Rust variant name.
const KIND: &str = "kind";

/// A Python attribute value for one field of an error variant.
enum Field {
    Str(String),
    OptStr(Option<String>),
    Int(u64),
    State(PyAsvoJobState),
}

/// Make an exception of type `T` with the message of `message`, and set
/// `kind` and `fields` on it.
fn build<T: pyo3::PyTypeInfo>(
    py: Python<'_>,
    message: String,
    kind: &str,
    fields: Vec<(&str, Field)>,
) -> PyErr {
    let err = PyErr::new::<T, _>(message);
    let value = err.value(py);
    let mut result = value.setattr(KIND, kind);
    for (name, field) in fields {
        result = result.and_then(|()| match field {
            Field::Str(s) => value.setattr(name, s),
            Field::OptStr(s) => value.setattr(name, s),
            Field::Int(n) => value.setattr(name, n),
            Field::State(s) => value.setattr(name, s),
        });
    }
    // Setting an attribute on a new exception instance does not fail in
    // practice; if it ever does, raise that error instead.
    match result {
        Ok(()) => err,
        Err(e) => e,
    }
}

/// Convert a library [`lib::AsvoApiError`] to a Python `AsvoApiError`.
///
/// `lib::AsvoApiError::InvalidParameter` (a job argument the schema does
/// not allow, found before any request) becomes a `ValueError`, as the
/// Python submit methods raise for the same fault when they build the
/// request body.
pub(crate) fn api_error(py: Python<'_>, e: lib::AsvoApiError) -> PyErr {
    let message = e.to_string();
    let (kind, fields) = match e {
        lib::AsvoApiError::MissingAuthKey => ("MissingAuthKey", vec![]),
        lib::AsvoApiError::AuthenticationFailed { message } => (
            "AuthenticationFailed",
            vec![("message", Field::Str(message))],
        ),
        lib::AsvoApiError::InvalidParameter { .. } => {
            return PyValueError::new_err(message);
        }
        lib::AsvoApiError::Conversion(_) => ("Conversion", vec![]),
        lib::AsvoApiError::BadJson(_) => ("BadJson", vec![]),
        lib::AsvoApiError::Reqwest(_) => ("Reqwest", vec![]),
        lib::AsvoApiError::ApiError {
            error_code,
            message,
            detail,
            suggestion,
        } => (
            "ApiError",
            vec![
                ("error_code", Field::Str(error_code)),
                ("message", Field::Str(message)),
                ("detail", Field::OptStr(detail)),
                ("suggestion", Field::OptStr(suggestion)),
            ],
        ),
        lib::AsvoApiError::BadStatus { code, message } => (
            "BadStatus",
            vec![
                ("code", Field::Int(u64::from(code.as_u16()))),
                ("message", Field::Str(message)),
            ],
        ),
    };
    build::<AsvoApiError>(py, message, kind, fields)
}

/// Convert a library [`lib::AsvoError`] to a Python exception.
///
/// `lib::AsvoError::AsvoApi` wraps an API error (for example, the job list
/// request before a download), so it becomes a Python `AsvoApiError`, and
/// the caller can catch every API failure in the same way.
pub(crate) fn asvo_error(py: Python<'_>, e: lib::AsvoError) -> PyErr {
    let message = e.to_string();
    let jobid = |id: u32| ("jobid", Field::Int(u64::from(id)));
    let obsid = |o: crate::Obsid| ("obsid", Field::Int(u64::from(o)));
    let (kind, fields) = match e {
        lib::AsvoError::AsvoApi(inner) => return api_error(py, inner),
        lib::AsvoError::NoAsvoJob(id) => ("NoAsvoJob", vec![jobid(id)]),
        lib::AsvoError::JobFailed {
            jobid: id,
            obsid: o,
            error,
        } => (
            "JobFailed",
            vec![jobid(id), obsid(o), ("error", Field::Str(error))],
        ),
        lib::AsvoError::JobExpired(id) => ("JobExpired", vec![jobid(id)]),
        lib::AsvoError::JobCancelled(id) => ("JobCancelled", vec![jobid(id)]),
        lib::AsvoError::NoObsid(o) => ("NoObsid", vec![obsid(o)]),
        lib::AsvoError::NoJobReadyForObsid(o) => ("NoJobReadyForObsid", vec![obsid(o)]),
        lib::AsvoError::TooManyObsids(o) => ("TooManyObsids", vec![obsid(o)]),
        lib::AsvoError::NotReady { jobid: id, state } => (
            "NotReady",
            vec![
                jobid(id),
                ("state", Field::State(PyAsvoJobState::from(&state))),
            ],
        ),
        lib::AsvoError::NoFiles(id) => ("NoFiles", vec![jobid(id)]),
        lib::AsvoError::HashMismatch {
            jobid: id,
            file,
            calculated_hash,
            expected_hash,
        } => (
            "HashMismatch",
            vec![
                jobid(id),
                ("file", Field::Str(file)),
                ("calculated_hash", Field::Str(calculated_hash)),
                ("expected_hash", Field::Str(expected_hash)),
            ],
        ),
        lib::AsvoError::InvalidJobState { str } => {
            ("InvalidJobState", vec![("str", Field::Str(str))])
        }
        lib::AsvoError::InvalidJobType { str } => {
            ("InvalidJobType", vec![("str", Field::Str(str))])
        }
        lib::AsvoError::Reqwest(_) => ("Reqwest", vec![]),
        lib::AsvoError::IO(_) => ("IO", vec![]),
        lib::AsvoError::Interrupted => ("Interrupted", vec![]),
        lib::AsvoError::NoUrl { job_id } => {
            ("NoUrl", vec![("job_id", Field::Int(u64::from(job_id)))])
        }
        lib::AsvoError::NoPath { job_id } => {
            ("NoPath", vec![("job_id", Field::Int(u64::from(job_id)))])
        }
        lib::AsvoError::HttpError { status, message } => (
            "HttpError",
            vec![
                ("status", Field::Int(u64::from(status))),
                ("message", Field::Str(message)),
            ],
        ),
        lib::AsvoError::Http404Error { job_id } => (
            "Http404Error",
            vec![("job_id", Field::Int(u64::from(job_id)))],
        ),
    };
    build::<AsvoError>(py, message, kind, fields)
}
