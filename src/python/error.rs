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
use pyo3::types::PyDict;

use super::types::PyJobState;
// The library's error enums have the same names as the Python exceptions,
// so they are used through this path.
use crate::asvo as lib;

// With the "python-stubgen" feature, pyo3-stub-gen's `create_exception!`
// wraps pyo3's and also registers the exception for the stubs.
#[cfg(not(feature = "python-stubgen"))]
use pyo3::create_exception;
#[cfg(feature = "python-stubgen")]
use pyo3_stub_gen::create_exception;

create_exception!(
    mwa_giant_squid,
    AsvoApiError,
    PyException,
    "An MWA ASVO API call failed. `kind` is the Rust `AsvoApiError` variant.\n\n`kind` is always set. The other attributes are set only for the kinds shown."
);

create_exception!(
    mwa_giant_squid,
    AsvoError,
    PyException,
    "A download or job check failed. `kind` is the Rust `AsvoError` variant.\n\n`kind` is always set. The other attributes are set only for the kinds shown. A failed API call during a download raises `AsvoApiError`, not this."
);

/// The attributes of the two exceptions, for the stubs. `build` sets them
/// at run time, so pyo3-stub-gen cannot see them.
#[cfg(feature = "python-stubgen")]
mod stub_attributes {
    use std::collections::HashMap;

    use pyo3_stub_gen::type_info::{MemberInfo, PyMethodsInfo};
    use pyo3_stub_gen::PyStubType;

    use super::super::types::PyJobState;
    use super::{AsvoApiError, AsvoError};

    /// One attribute: its name, its Rust type (for the Python type) and its
    /// docstring.
    macro_rules! attr {
        ($name:literal, $ty:ty, $doc:literal) => {
            MemberInfo {
                name: $name,
                r#type: <$ty as PyStubType>::type_output,
                doc: $doc,
                default: None,
                deprecated: None,
            }
        };
    }

    pyo3_stub_gen::inventory::submit! {
        PyMethodsInfo {
            struct_id: std::any::TypeId::of::<AsvoApiError>,
            attrs: &[
                attr!("kind", String, "\"MissingAuthKey\", \"AuthenticationFailed\", \"Conversion\", \"BadJson\", \"Reqwest\", \"ApiError\" or \"BadStatus\"."),
                attr!("message", String, "AuthenticationFailed, ApiError and BadStatus."),
                attr!("error_code", String, "ApiError: the server's machine-readable error code."),
                attr!("detail", Option<String>, "ApiError."),
                attr!("suggestion", Option<String>, "ApiError."),
                attr!("field_errors", Vec<HashMap<String, String>>, "ApiError: the fields that failed validation, each as `{\"field\": ..., \"message\": ...}`. Can be empty."),
                attr!("request_id", Option<String>, "ApiError: the server's ID for the request, for a support request."),
                attr!("code", u16, "BadStatus: the HTTP status code."),
            ],
            getters: &[],
            setters: &[],
            methods: &[],
            file: file!(),
            line: line!(),
            column: column!(),
        }
    }

    pyo3_stub_gen::inventory::submit! {
        PyMethodsInfo {
            struct_id: std::any::TypeId::of::<AsvoError>,
            attrs: &[
                attr!("kind", String, "The variant, for example \"NoAsvoJob\", \"JobFailed\", \"NotReady\", \"HashMismatch\" or \"Interrupted\"."),
                attr!("job_id", u64, "NoAsvoJob, JobFailed, JobCancelled, NotReady, NoFiles, HashMismatch, NoUrl, NoPath and Http404Error."),
                attr!("obs_id", u64, "JobFailed, NoObsId, NoJobReadyForObsId and TooManyObsIds."),
                attr!("error", String, "JobFailed: the job's error message."),
                attr!("error_code", Option<i64>, "JobFailed: the job's error code, or None."),
                attr!("job_state", PyJobState, "NotReady: the job's state."),
                attr!("file", String, "HashMismatch."),
                attr!("calculated_hash", String, "HashMismatch."),
                attr!("expected_hash", String, "HashMismatch."),
                attr!("status", u16, "HttpError: the HTTP status code."),
                attr!("message", String, "HttpError."),
                attr!("str", String, "InvalidJobState and InvalidJobType: the text that could not be parsed."),
                attr!("name", String, "InvalidEnvironment: the name of the environment variable."),
                attr!("value", String, "InvalidEnvironment: its value."),
                attr!("problem", String, "InvalidEnvironment and InvalidJob: what is wrong."),
                attr!("id", i64, "InvalidJob: the `id` of the job, as the API gave it."),
            ],
            getters: &[],
            setters: &[],
            methods: &[],
            file: file!(),
            line: line!(),
            column: column!(),
        }
    }
}

/// The name of the attribute that holds the Rust variant name.
const KIND: &str = "kind";

/// A Python attribute value for one field of an error variant.
enum Field {
    Str(String),
    OptStr(Option<String>),
    Int(u64),
    OptInt(Option<i64>),
    State(PyJobState),
    /// A list of `{"field": ..., "message": ...}` dicts.
    ErrorDicts(Vec<lib::apiv2::openapi::FieldError>),
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
            Field::OptInt(n) => value.setattr(name, n),
            Field::State(s) => value.setattr(name, s),
            Field::ErrorDicts(errors) => {
                let list = errors
                    .into_iter()
                    .map(|e| {
                        let dict = PyDict::new(py);
                        dict.set_item("field", e.field)?;
                        dict.set_item("message", e.message)?;
                        Ok(dict)
                    })
                    .collect::<PyResult<Vec<_>>>();
                list.and_then(|list| value.setattr(name, list))
            }
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
            field_errors,
            request_id,
        } => (
            "ApiError",
            vec![
                ("error_code", Field::Str(error_code)),
                ("message", Field::Str(message)),
                ("detail", Field::OptStr(detail)),
                ("suggestion", Field::OptStr(suggestion)),
                ("field_errors", Field::ErrorDicts(field_errors)),
                ("request_id", Field::OptStr(request_id)),
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
    let job_id = |id: crate::AsvoJobId| ("job_id", Field::Int(id));
    let obs_id = |o: crate::ObsId| ("obs_id", Field::Int(u64::from(o)));
    let (kind, fields) = match e {
        lib::AsvoError::AsvoApi(inner) => return api_error(py, inner),
        lib::AsvoError::NoAsvoJob(id) => ("NoAsvoJob", vec![job_id(id)]),
        lib::AsvoError::JobFailed {
            job_id: id,
            obs_id: o,
            error,
            error_code,
        } => (
            "JobFailed",
            vec![
                job_id(id),
                obs_id(o),
                ("error", Field::Str(error)),
                ("error_code", Field::OptInt(error_code)),
            ],
        ),
        lib::AsvoError::InvalidJob { id, problem } => (
            "InvalidJob",
            vec![
                ("id", Field::OptInt(Some(id))),
                ("problem", Field::Str(problem)),
            ],
        ),
        lib::AsvoError::JobCancelled(id) => ("JobCancelled", vec![job_id(id)]),
        lib::AsvoError::NoObsId(o) => ("NoObsId", vec![obs_id(o)]),
        lib::AsvoError::NoJobReadyForObsId(o) => ("NoJobReadyForObsId", vec![obs_id(o)]),
        lib::AsvoError::TooManyObsIds(o) => ("TooManyObsIds", vec![obs_id(o)]),
        lib::AsvoError::NotReady {
            job_id: id,
            job_state,
        } => (
            "NotReady",
            vec![
                job_id(id),
                ("job_state", Field::State(PyJobState::from(job_state))),
            ],
        ),
        lib::AsvoError::NoFiles(id) => ("NoFiles", vec![job_id(id)]),
        lib::AsvoError::HashMismatch {
            job_id: id,
            file,
            calculated_hash,
            expected_hash,
        } => (
            "HashMismatch",
            vec![
                job_id(id),
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
        lib::AsvoError::InvalidEnvironment {
            name,
            value,
            problem,
        } => (
            "InvalidEnvironment",
            vec![
                ("name", Field::Str(name)),
                ("value", Field::Str(value)),
                ("problem", Field::Str(problem)),
            ],
        ),
        lib::AsvoError::Reqwest(_) => ("Reqwest", vec![]),
        lib::AsvoError::IO(_) => ("IO", vec![]),
        lib::AsvoError::Interrupted => ("Interrupted", vec![]),
        lib::AsvoError::NoUrl { job_id } => ("NoUrl", vec![("job_id", Field::Int(job_id))]),
        lib::AsvoError::NoPath { job_id } => ("NoPath", vec![("job_id", Field::Int(job_id))]),
        lib::AsvoError::HttpError { status, message } => (
            "HttpError",
            vec![
                ("status", Field::Int(u64::from(status))),
                ("message", Field::Str(message)),
            ],
        ),
        lib::AsvoError::Http404Error { job_id } => {
            ("Http404Error", vec![("job_id", Field::Int(job_id))])
        }
    };
    build::<AsvoError>(py, message, kind, fields)
}
