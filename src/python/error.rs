// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python exceptions `AsvoApiError` and `AsvoError`.
//!
//! Each exception has the name of the Rust error enum. Its `kind` attribute
//! is the name of the Rust variant (for example `"ApiError"`), and it has one
//! attribute for each of that variant's fields. The exception's message is
//! the Rust error's message.

use std::path::Path;

use pyo3::exceptions::{PyException, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::types::{PyJobState, PyType};
// The library's error enums have the same names as the Python exceptions,
// so they are used through this path.
use crate::mwa_asvo as lib;

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
    "A download or job check failed. `kind` is the Rust `AsvoError` variant.\n\n`kind` is always set. The other attributes are set only for the kinds shown. A failed API call during a download raises `AsvoApiError`, not this. A file or directory that cannot be read, written or made raises `OSError` (`FileNotFoundError`, `PermissionError` and so on), not this."
);

/// The attributes of the two exceptions, for the stubs. `build` sets them
/// at run time, so pyo3-stub-gen cannot see them.
#[cfg(feature = "python-stubgen")]
mod stub_attributes {
    use std::collections::HashMap;

    use pyo3_stub_gen::type_info::{MemberInfo, PyMethodsInfo};
    use pyo3_stub_gen::PyStubType;

    use super::super::types::{PyJobState, PyType};
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
                attr!("kind", String, "\"MissingAuthKey\" (the `api_key` given to `AsvoClient` is empty), \"AuthenticationFailed\", \"Conversion\", \"BadJson\", \"Reqwest\", \"ApiError\" or \"BadStatus\". An argument outside the MWA ASVO limits raises `ValueError`, not this."),
                attr!("message", String, "AuthenticationFailed, ApiError and BadStatus."),
                attr!("error_code", Option<String>, "ApiError and AuthenticationFailed: the server's machine-readable error code. Always set for ApiError; None for an AuthenticationFailed whose reply was not an MWA ASVO ErrorResponse."),
                attr!("detail", Option<String>, "ApiError and AuthenticationFailed."),
                attr!("suggestion", Option<String>, "ApiError and AuthenticationFailed."),
                attr!("field_errors", Vec<HashMap<String, String>>, "ApiError and AuthenticationFailed: the fields that failed validation, each as `{\"field\": ..., \"message\": ...}`. Can be empty."),
                attr!("request_id", Option<String>, "ApiError and AuthenticationFailed: the server's ID for the request, for a support request."),
                attr!("status", u16, "ApiError and BadStatus: the HTTP status code (as `status` of an AsvoError HttpError)."),
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
                attr!("job_id", u64, "NoAsvoJob, JobFailed, JobCancelled, NotReady, NoFiles, JobExpired, HashMismatch, NoUrl, NoPath, NoHash, FilesNotReachable, DestinationExists, HttpError and Http404Error."),
                attr!("obs_id", u64, "JobFailed, NoObsId, NoJobReadyForObsId and TooManyObsIds."),
                attr!("expired", Option<jiff::Timestamp>, "JobExpired: when the download links expired (the `Expires` of the URL), or None if it is not known."),
                attr!("error", String, "JobFailed: the job's error message."),
                attr!("error_code", Option<i64>, "JobFailed: the job's error code, or None."),
                attr!("job_state", PyJobState, "NotReady: the job's state."),
                attr!("delivery", PyType, "FilesNotReachable: where the MWA ASVO delivered the files (Dug or Scratch)."),
                attr!("path", Option<String>, "FilesNotReachable: the path of the files, if the MWA ASVO gave one."),
                attr!("file", String, "HashMismatch: the file that failed the check. SymlinkInDownloadDir: the path of the symbolic link. DestinationExists: the path that is already there."),
                attr!("calculated_hash", String, "HashMismatch."),
                attr!("expected_hash", String, "HashMismatch."),
                attr!("status", u16, "HttpError: the HTTP status code."),
                attr!("message", String, "HttpError."),
                attr!("str", String, "InvalidJobState and InvalidJobType: the text that could not be parsed."),
                attr!("name", String, "InvalidEnvironment: the name of the environment variable. (The module reads no environment variables, so it does not raise InvalidEnvironment.)"),
                attr!("value", String, "InvalidEnvironment: its value."),
                attr!("problem", String, "InvalidEnvironment and InvalidJob: what is wrong. (A job list skips an InvalidJob job with a warning, so the module does not raise it.)"),
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
    /// A delivery type (`Type`), or `None`.
    Delivery(PyType),
    /// A list of `{"field": ..., "message": ...}` dicts.
    ErrorDicts(Vec<lib::api::openapi::FieldError>),
    /// A time (a `datetime`), or `None`.
    Time(Option<jiff::Timestamp>),
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
            Field::Delivery(d) => value.setattr(name, d),
            Field::Time(t) => value.setattr(name, t),
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

/// The exception fields that come from the reply of the server (an
/// `ErrorResponse`), other than the message and the error code. With no
/// reply, they are empty.
fn reply_fields(
    response: Option<Box<lib::api::openapi::ErrorResponse>>,
) -> Vec<(&'static str, Field)> {
    let (detail, suggestion, field_errors, request_id) = match response {
        Some(r) => (
            r.detail,
            r.suggestion,
            r.field_errors.unwrap_or_default(),
            r.request_id,
        ),
        None => (None, None, Vec::new(), None),
    };
    vec![
        ("detail", Field::OptStr(detail)),
        ("suggestion", Field::OptStr(suggestion)),
        ("field_errors", Field::ErrorDicts(field_errors)),
        ("request_id", Field::OptStr(request_id)),
    ]
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
        lib::AsvoApiError::MissingAuthKey { .. } => ("MissingAuthKey", vec![]),
        lib::AsvoApiError::AuthenticationFailed { message, response } => {
            let error_code = response.as_ref().map(|r| r.error_code.clone());
            let mut fields = vec![
                ("message", Field::Str(message)),
                ("error_code", Field::OptStr(error_code)),
            ];
            fields.extend(reply_fields(response));
            ("AuthenticationFailed", fields)
        }
        lib::AsvoApiError::InvalidParameter { .. } => {
            return PyValueError::new_err(message);
        }
        lib::AsvoApiError::Conversion(_) => ("Conversion", vec![]),
        lib::AsvoApiError::BadJson(_) => ("BadJson", vec![]),
        lib::AsvoApiError::Reqwest(_) => ("Reqwest", vec![]),
        lib::AsvoApiError::ApiError { status, response } => {
            let mut fields = vec![
                ("status", Field::Int(u64::from(status))),
                ("error_code", Field::Str(response.error_code.clone())),
                ("message", Field::Str(response.message.clone())),
            ];
            fields.extend(reply_fields(Some(response)));
            ("ApiError", fields)
        }
        lib::AsvoApiError::BadStatus { status, message } => (
            "BadStatus",
            vec![
                ("status", Field::Int(u64::from(status))),
                ("message", Field::Str(message)),
            ],
        ),
    };
    build::<AsvoApiError>(py, message, kind, fields)
}

/// The `OSError` for a file that cannot be read, made as Python's `open`
/// makes it: `OSError(errno, strerror, filename)`, which Python turns into
/// the subclass for the error (`FileNotFoundError`, `PermissionError` and so
/// on). An error with no error number (for example, text that is not UTF-8)
/// is the error that pyo3 makes, with the file name set.
pub(crate) fn os_error(py: Python<'_>, path: &Path, source: std::io::Error) -> PyErr {
    let filename = path.to_string_lossy().into_owned();
    let Some(errno) = source.raw_os_error() else {
        let err = PyErr::from(source);
        // Setting an attribute on a new exception instance does not fail in
        // practice; if it ever does, raise that error instead.
        return match err.value(py).setattr("filename", filename) {
            Ok(()) => err,
            Err(e) => e,
        };
    };
    let make = || -> PyResult<PyErr> {
        let strerror = py.import("os")?.call_method1("strerror", (errno,))?;
        let exception = py
            .get_type::<PyOSError>()
            .call1((errno, strerror, filename))?;
        Ok(PyErr::from_value(exception))
    };
    make().unwrap_or_else(|e| e)
}

/// Convert a library [`lib::AsvoError`] to a Python exception.
///
/// `lib::AsvoError::AsvoApi` wraps an API error (for example, the job list
/// request before a download), so it becomes a Python `AsvoApiError`, and
/// the caller can catch every API failure in the same way.
/// `lib::AsvoError::InvalidOption` (a download option that cannot be used,
/// found before any request) becomes a `ValueError`, like an argument that
/// the API does not allow.
/// `lib::AsvoError::IO` and `lib::AsvoError::CannotCreate` (a file or a
/// directory that cannot be read, written or made) become an `OSError`, as
/// Python's `open` raises it: a `FileNotFoundError`, a `PermissionError` and
/// so on. `CannotCreate` has the path as the `filename`.
pub(crate) fn asvo_error(py: Python<'_>, e: lib::AsvoError) -> PyErr {
    let message = e.to_string();
    let job_id = |id: crate::AsvoJobId| ("job_id", Field::Int(id.get()));
    let obs_id = |o: crate::ObsId| ("obs_id", Field::Int(u64::from(o)));
    let (kind, fields) = match e {
        lib::AsvoError::AsvoApi(inner) => return api_error(py, inner),
        lib::AsvoError::InvalidOption { .. } => return PyValueError::new_err(message),
        lib::AsvoError::IO(source) => return PyErr::from(source),
        lib::AsvoError::CannotCreate { path, source } => return os_error(py, &path, source),
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
                ("id", Field::OptInt(i64::try_from(id.get()).ok())),
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
        lib::AsvoError::JobExpired {
            job_id: id,
            expired,
        } => (
            "JobExpired",
            vec![job_id(id), ("expired", Field::Time(expired))],
        ),
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
        lib::AsvoError::Interrupted => ("Interrupted", vec![]),
        lib::AsvoError::NoUrl { job_id: id } => ("NoUrl", vec![job_id(id)]),
        lib::AsvoError::NoPath { job_id: id } => ("NoPath", vec![job_id(id)]),
        lib::AsvoError::NoHash { job_id: id } => ("NoHash", vec![job_id(id)]),
        lib::AsvoError::FilesNotReachable {
            job_id: id,
            delivery,
            path,
        } => (
            "FilesNotReachable",
            vec![
                job_id(id),
                ("delivery", Field::Delivery(PyType::from(delivery))),
                ("path", Field::OptStr(path)),
            ],
        ),
        lib::AsvoError::SymlinkInDownloadDir { path } => (
            "SymlinkInDownloadDir",
            vec![("file", Field::Str(path.display().to_string()))],
        ),
        lib::AsvoError::DestinationExists { job_id: id, path } => (
            "DestinationExists",
            vec![job_id(id), ("file", Field::Str(path.display().to_string()))],
        ),
        lib::AsvoError::HttpError {
            job_id: id,
            status,
            message,
        } => (
            "HttpError",
            vec![
                job_id(id),
                ("status", Field::Int(u64::from(status))),
                ("message", Field::Str(message)),
            ],
        ),
        lib::AsvoError::Http404Error { job_id: id } => ("Http404Error", vec![job_id(id)]),
    };
    build::<AsvoError>(py, message, kind, fields)
}
