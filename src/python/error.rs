// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python exceptions `MwaAsvoApiError`, `MwaAsvoError`, `ObsIdError` and
//! `ParseError`.
//!
//! Each exception has the name of the Rust error enum. Its `kind` attribute
//! is the name of the Rust variant (for example `"ApiError"`), and it has one
//! attribute for each of that variant's fields. The exception's message is
//! the Rust error's message.

use std::path::Path;

use pyo3::exceptions::{PyBaseException, PyException, PyOSError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::obs_id::PyObsId;
use super::types::{PyDeliveryDestination, PyJobState};
// The library's error enums have the same names as the Python exceptions,
// so they are used through this path.
use crate::mwa_asvo as lib;
use crate::obs_id::{ObsId, ObsIdError as LibObsIdError};
use crate::ParseError as LibParseError;

// With the "python-stubgen" feature, pyo3-stub-gen's `create_exception!`
// wraps pyo3's and also registers the exception for the stubs.
#[cfg(not(feature = "python-stubgen"))]
use pyo3::create_exception;
#[cfg(feature = "python-stubgen")]
use pyo3_stub_gen::create_exception;

create_exception!(
    mwa_giant_squid,
    MwaAsvoApiError,
    PyException,
    "An MWA ASVO API call failed, or an argument is outside what the MWA ASVO API allows. `kind` is the Rust `MwaAsvoApiError` variant, and the exception is also an instance of the class for that variant (for example `MwaAsvoApiErrorInvalidParameter`).\n\n`kind` is always set. The other attributes are set only for the kinds shown."
);

create_exception!(
    mwa_giant_squid,
    MwaAsvoError,
    PyException,
    "A download or job check failed. `kind` is the Rust `MwaAsvoError` variant, and the exception is also an instance of the class for that variant (for example `MwaAsvoErrorNoMwaAsvoJob`).\n\n`kind` is always set. The other attributes are set only for the kinds shown. A failed API call during a download is `MwaAsvoErrorMwaAsvoApi`, which has the `MwaAsvoApiError` as its `source`. A file or directory that cannot be read, written or made raises `OSError` (`FileNotFoundError`, `PermissionError` and so on), not this."
);

create_exception!(
    mwa_giant_squid,
    ObsIdError,
    PyException,
    "A number or text is not a valid Obs ID. `kind` is the Rust `ObsIdError` variant, and the exception is also an instance of the class for that variant (for example `ObsIdErrorWrongNumDigits`).\n\n`kind` is always set. The other attributes are set only for the kinds shown."
);

create_exception!(
    mwa_giant_squid,
    ParseError,
    PyException,
    "Job IDs, Obs IDs or a time could not be read. `kind` is the Rust `ParseError` variant, and the exception is also an instance of the class for that variant (for example `ParseErrorNoObsIds`).\n\n`kind` is always set. The other attributes are set only for the kinds shown. A file that cannot be read raises `OSError` (`FileNotFoundError`, `PermissionError` and so on), not this."
);

/// Make the exception class of each variant of an error: a subclass of the
/// error's exception, which has the names of the error and the variant
/// (`ParseErrorNoObsIds`).
///
/// With the "python-stubgen" feature, the class is also registered for the
/// stubs, with the error's exception as its base. (pyo3-stub-gen's
/// `create_exception!` writes the name of a base that is not a built-in
/// exception with the module `builtins`, which is wrong.)
macro_rules! variant_exceptions {
    ($base:ident: $($name:ident => $doc:literal,)+) => {
        $(
            pyo3::create_exception!(mwa_giant_squid, $name, $base, $doc);

            #[cfg(feature = "python-stubgen")]
            impl pyo3_stub_gen::PyStubType for $name {
                fn type_output() -> pyo3_stub_gen::TypeInfo {
                    pyo3_stub_gen::TypeInfo::unqualified(stringify!($name))
                }
            }

            #[cfg(feature = "python-stubgen")]
            pyo3_stub_gen::impl_py_runtime_type!($name);

            #[cfg(feature = "python-stubgen")]
            pyo3_stub_gen::inventory::submit! {
                pyo3_stub_gen::type_info::PyClassInfo {
                    pyclass_name: stringify!($name),
                    struct_id: std::any::TypeId::of::<$name>,
                    getters: &[],
                    setters: &[],
                    module: Some("mwa_giant_squid"),
                    doc: $doc,
                    bases: &[|| pyo3_stub_gen::TypeInfo::unqualified(stringify!($base))],
                    has_eq: false,
                    has_ord: false,
                    has_hash: false,
                    has_str: false,
                    subclass: true,
                }
            }
        )+
    };
}

pub(super) use variant_exceptions;

variant_exceptions! {
    MwaAsvoApiError:
    MwaAsvoApiErrorMissingAuthKey => "No API key was given: it is empty (kind `MissingAuthKey`).",
    MwaAsvoApiErrorAuthenticationFailed => "The MWA ASVO refused the login (kind `AuthenticationFailed`).",
    MwaAsvoApiErrorConversion => "A value could not be turned into the type that the schema has (kind `Conversion`).",
    MwaAsvoApiErrorInvalidParameter => "A job argument is outside what the MWA ASVO OpenAPI schema allows. It was found before any request was sent (kind `InvalidParameter`). `name` is the name of the argument, and `message` says what is wrong.",
    MwaAsvoApiErrorBadJson => "The JSON of an MWA ASVO reply could not be decoded (kind `BadJson`).",
    MwaAsvoApiErrorReqwest => "An HTTP request failed: no connection, a timeout, a TLS problem and so on (kind `Reqwest`).",
    MwaAsvoApiErrorApiError => "The MWA ASVO replied with an error (kind `ApiError`). `status` is the HTTP status, and `error_code` is the server's error code.",
    MwaAsvoApiErrorBadStatus => "The MWA ASVO replied with an HTTP status that is not a success, and a body that is not an MWA ASVO error (kind `BadStatus`).",
}

variant_exceptions! {
    MwaAsvoError:
    MwaAsvoErrorNoMwaAsvoJob => "There is no job with this Job ID in the job list (kind `NoMwaAsvoJob`).",
    MwaAsvoErrorJobFailed => "The job is in the `Error` state (kind `JobFailed`). `error` is its message.",
    MwaAsvoErrorInvalidJob => "The server gave a job that cannot be used (kind `InvalidJob`).",
    MwaAsvoErrorJobCancelled => "The job was cancelled (kind `JobCancelled`).",
    MwaAsvoErrorNoObsId => "No job has this Obs ID (kind `NoObsId`).",
    MwaAsvoErrorNoJobReadyForObsId => "No job for this Obs ID is ready for download (kind `NoJobReadyForObsId`).",
    MwaAsvoErrorTooManyObsIds => "More than one job for this Obs ID is ready for download (kind `TooManyObsIds`).",
    MwaAsvoErrorNotReady => "The job is not ready for download (kind `NotReady`). `job_state` is its state.",
    MwaAsvoErrorNoFiles => "The job has no files (kind `NoFiles`).",
    MwaAsvoErrorHashMismatch => "The SHA-1 hash of a downloaded file is not the MWA ASVO's (kind `HashMismatch`).",
    MwaAsvoErrorInvalidJobState => "The text is not the name of a job state (kind `InvalidJobState`).",
    MwaAsvoErrorInvalidJobType => "The text is not the name of a job type (kind `InvalidJobType`).",
    MwaAsvoErrorInvalidEnvironment => "An environment variable has a value that cannot be used (kind `InvalidEnvironment`).",
    MwaAsvoErrorInvalidOption => "A download option has a value that cannot be used, for example a `buffer_size` of 0. It was found before any request was sent (kind `InvalidOption`). `name` is the name of the option, and `message` says what is wrong.",
    MwaAsvoErrorMwaAsvoApi => "An MWA ASVO API call failed during a download or job check, for example the job list request that a download does first (kind `MwaAsvoApi`). `source` is the `MwaAsvoApiError`, which is also the `__cause__`.",
    MwaAsvoErrorInterrupted => "The download was stopped (kind `Interrupted`).",
    MwaAsvoErrorNoUrl => "The job's file has no download URL (kind `NoUrl`).",
    MwaAsvoErrorNoHash => "The job's file has no hash (kind `NoHash`).",
    MwaAsvoErrorFilesNotReachable => "The files are on a file system that this computer cannot reach (kind `FilesNotReachable`).",
    MwaAsvoErrorNoPath => "The job's files have no path (kind `NoPath`).",
    MwaAsvoErrorSymlinkInDownloadDir => "The download directory has a symbolic link where a file should be written (kind `SymlinkInDownloadDir`).",
    MwaAsvoErrorDestinationExists => "The place for the files of a Scratch job is already there (kind `DestinationExists`).",
    MwaAsvoErrorHttpError => "The download server replied with an HTTP error (kind `HttpError`).",
    MwaAsvoErrorJobExpired => "The download links of the job have expired (kind `JobExpired`). Resubmit the job.",
    MwaAsvoErrorHttp404Error => "The download server did not find the file (kind `Http404Error`).",
}

variant_exceptions! {
    ObsIdError:
    ObsIdErrorWrongNumDigits => "The number is outside `ObsId.MIN` to `ObsId.MAX` (kind `WrongNumDigits`). `value` is the number.",
    ObsIdErrorParse => "The text is not a number (kind `Parse`).",
}

variant_exceptions! {
    ParseError:
    ParseErrorInsideFile => "A word in a file is not a number (kind `InsideFile`). `file` and `text` say where.",
    ParseErrorInsideStdin => "A word on standard input is not a number (kind `InsideStdin`). Only the command reads standard input, so the module does not raise this.",
    ParseErrorStdinError => "Standard input has the error line of a command that failed (kind `StdinError`). Only the command reads standard input, so the module does not raise this.",
    ParseErrorStdinJson => "Standard input is JSON, but not the output of `giant-squid list --json` (kind `StdinJson`). Only the command reads standard input, so the module does not raise this.",
    ParseErrorJobIdsGiven => "Only Obs IDs were expected, but Job IDs were given (kind `JobIdsGiven`). `job_ids` are the Job IDs.",
    ParseErrorObsIdsGiven => "Only Job IDs were expected, but Obs IDs were given (kind `ObsIdsGiven`). `obs_ids` are the Obs IDs.",
    ParseErrorZeroJobId => "0 is not a Job ID or an Obs ID (kind `ZeroJobId`).",
    ParseErrorInvalidObsId => "A number has 10 digits, so it is read as an Obs ID, but it is below `ObsId.MIN` (kind `InvalidObsId`). `value` is the number.",
    ParseErrorNoObsIds => "No Obs IDs were given (kind `NoObsIds`).",
    ParseErrorNoJobIds => "No Job IDs were given (kind `NoJobIds`).",
    ParseErrorInvalidTime => "The text is not a time (kind `InvalidTime`): use RFC 3339 (`2026-09-01T00:00:00Z`) or a date (`2026-09-01`).",
}

/// The attributes of the two exceptions, for the stubs. `build` sets them
/// at run time, so pyo3-stub-gen cannot see them.
#[cfg(feature = "python-stubgen")]
mod stub_attributes {
    use std::collections::HashMap;

    use pyo3_stub_gen::type_info::{MemberInfo, PyMethodsInfo};
    use pyo3_stub_gen::PyStubType;

    use super::super::obs_id::PyObsId;
    use super::super::types::{PyDeliveryDestination, PyJobState};
    use super::{MwaAsvoApiError, MwaAsvoError, ObsIdError, ParseError};

    /// The type of an `MwaAsvoApiError` object, for the stub of `source`.
    /// (pyo3-stub-gen writes the name of an exception that it makes with the
    /// module `builtins`, which is wrong.)
    struct ApiErrorObject;

    impl PyStubType for ApiErrorObject {
        fn type_output() -> pyo3_stub_gen::TypeInfo {
            pyo3_stub_gen::TypeInfo::unqualified("MwaAsvoApiError")
        }
    }

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
            struct_id: std::any::TypeId::of::<MwaAsvoApiError>,
            attrs: &[
                attr!("kind", String, "\"MissingAuthKey\" (the `api_key` given to `MwaAsvoClient` is empty), \"AuthenticationFailed\", \"Conversion\", \"InvalidParameter\" (an argument is outside the MWA ASVO limits), \"BadJson\", \"Reqwest\", \"ApiError\" or \"BadStatus\"."),
                attr!("name", String, "InvalidParameter: the name of the argument, as the OpenAPI schema has it."),
                attr!("message", String, "AuthenticationFailed, InvalidParameter, ApiError and BadStatus."),
                attr!("error_code", Option<String>, "ApiError and AuthenticationFailed: the server's machine-readable error code. Always set for ApiError; None for an AuthenticationFailed whose reply was not an MWA ASVO ErrorResponse."),
                attr!("detail", Option<String>, "ApiError and AuthenticationFailed."),
                attr!("suggestion", Option<String>, "ApiError and AuthenticationFailed."),
                attr!("field_errors", Vec<HashMap<String, String>>, "ApiError and AuthenticationFailed: the fields that failed validation, each as `{\"field\": ..., \"message\": ...}`. Can be empty."),
                attr!("request_id", Option<String>, "ApiError and AuthenticationFailed: the server's ID for the request, for a support request."),
                attr!("status", u16, "ApiError and BadStatus: the HTTP status code (as `status` of an MwaAsvoError HttpError)."),
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
            struct_id: std::any::TypeId::of::<MwaAsvoError>,
            attrs: &[
                attr!("kind", String, "The variant, for example \"NoMwaAsvoJob\", \"JobFailed\", \"NotReady\", \"HashMismatch\" or \"Interrupted\"."),
                attr!("job_id", u64, "NoMwaAsvoJob, JobFailed, JobCancelled, NotReady, NoFiles, JobExpired, HashMismatch, NoUrl, NoPath, NoHash, FilesNotReachable, DestinationExists, HttpError and Http404Error."),
                attr!("obs_id", PyObsId, "JobFailed, NoObsId, NoJobReadyForObsId and TooManyObsIds."),
                attr!("expired", Option<jiff::Timestamp>, "JobExpired: when the download links expired (the `Expires` of the URL), or None if it is not known."),
                attr!("error", String, "JobFailed: the job's error message."),
                attr!("error_code", Option<i64>, "JobFailed: the job's error code, or None."),
                attr!("job_state", PyJobState, "NotReady: the job's state."),
                attr!("delivery", PyDeliveryDestination, "FilesNotReachable: where the MWA ASVO delivered the files (Dug or Scratch)."),
                attr!("path", Option<String>, "FilesNotReachable: the path of the files, if the MWA ASVO gave one."),
                attr!("file", String, "HashMismatch: the file that failed the check. SymlinkInDownloadDir: the path of the symbolic link. DestinationExists: the path that is already there."),
                attr!("calculated_hash", String, "HashMismatch."),
                attr!("expected_hash", String, "HashMismatch."),
                attr!("status", u16, "HttpError: the HTTP status code."),
                attr!("message", String, "HttpError and InvalidOption: what is wrong."),
                attr!("str", String, "InvalidJobState and InvalidJobType: the text that could not be parsed."),
                attr!("name", String, "InvalidEnvironment: the name of the environment variable. InvalidOption: the name of the option."),
                attr!("source", ApiErrorObject, "MwaAsvoApi: the API error that stopped the download or the job check. It is also the `__cause__`."),
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

    pyo3_stub_gen::inventory::submit! {
        PyMethodsInfo {
            struct_id: std::any::TypeId::of::<ObsIdError>,
            attrs: &[
                attr!("kind", String, "\"WrongNumDigits\" or \"Parse\"."),
                attr!("value", u64, "WrongNumDigits: the number."),
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
            struct_id: std::any::TypeId::of::<ParseError>,
            attrs: &[
                attr!("kind", String, "The variant, for example \"NoObsIds\", \"JobIdsGiven\" or \"InvalidTime\"."),
                attr!("file", String, "InsideFile: the file."),
                attr!("text", String, "InsideFile and InsideStdin: the word that is not a number."),
                attr!("message", String, "StdinError: the message of the error."),
                attr!("problem", String, "StdinJson: what is wrong."),
                attr!("job_ids", Vec<u64>, "JobIdsGiven: the Job IDs."),
                attr!("obs_ids", Vec<PyObsId>, "ObsIdsGiven: the Obs IDs."),
                attr!("value", u64, "InvalidObsId: the number."),
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
pub(super) const KIND: &str = "kind";

/// A Python attribute value for one field of an error variant.
enum Field {
    Str(String),
    OptStr(Option<String>),
    Int(u64),
    /// An Obs ID (`ObsId`).
    ObsId(ObsId),
    /// A list of Job IDs, as `int`.
    Ints(Vec<u64>),
    /// A list of Obs IDs (`ObsId`).
    ObsIds(Vec<ObsId>),
    OptInt(Option<i64>),
    State(PyJobState),
    /// A delivery (`DeliveryDestination`), or `None`.
    Delivery(PyDeliveryDestination),
    /// A list of `{"field": ..., "message": ...}` dicts.
    ErrorDicts(Vec<lib::api::openapi::FieldError>),
    /// A time (a `datetime`), or `None`.
    Time(Option<jiff::Timestamp>),
    /// An exception object.
    Exception(Py<PyBaseException>),
}

/// Make an exception of type `T` with the message of `message`, and set
/// `kind` and `fields` on it.
fn build<T: pyo3::PyTypeInfo>(
    py: Python<'_>,
    message: String,
    kind: &str,
    fields: Vec<(&str, Field)>,
) -> PyErr {
    build_from(py, PyErr::new::<T, _>(message), kind, fields)
}

/// Set `kind` and `fields` on the exception `err`, and return it.
fn build_from(py: Python<'_>, err: PyErr, kind: &str, fields: Vec<(&str, Field)>) -> PyErr {
    let value = err.value(py);
    let mut result = value.setattr(KIND, kind);
    for (name, field) in fields {
        result = result.and_then(|()| match field {
            Field::Str(s) => value.setattr(name, s),
            Field::OptStr(s) => value.setattr(name, s),
            Field::Int(n) => value.setattr(name, n),
            Field::ObsId(o) => value.setattr(name, PyObsId(o)),
            Field::Ints(ids) => value.setattr(name, ids),
            Field::ObsIds(ids) => {
                value.setattr(name, ids.into_iter().map(PyObsId).collect::<Vec<_>>())
            }
            Field::OptInt(n) => value.setattr(name, n),
            Field::State(s) => value.setattr(name, s),
            Field::Delivery(d) => value.setattr(name, d),
            Field::Time(t) => value.setattr(name, t),
            Field::Exception(e) => value.setattr(name, e),
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

/// Convert a library [`lib::MwaAsvoApiError`] to a Python `MwaAsvoApiError`: the
/// class of its variant.
pub(crate) fn api_error(py: Python<'_>, e: lib::MwaAsvoApiError) -> PyErr {
    api_error_ref(py, &e)
}

/// As [`api_error`], for an error that is borrowed (for example the error
/// that the library gives to the `on_result` of `submit_csv_jobs`).
pub(crate) fn api_error_ref(py: Python<'_>, e: &lib::MwaAsvoApiError) -> PyErr {
    let message = e.to_string();
    // The exception of a variant, with `kind` and `fields`.
    macro_rules! raise {
        ($class:ident, $kind:literal, $fields:expr) => {
            build::<$class>(py, message, $kind, $fields)
        };
    }
    match e {
        lib::MwaAsvoApiError::MissingAuthKey { .. } => {
            raise!(MwaAsvoApiErrorMissingAuthKey, "MissingAuthKey", vec![])
        }
        lib::MwaAsvoApiError::AuthenticationFailed {
            message: text,
            response,
        } => {
            let error_code = response.as_ref().map(|r| r.error_code.clone());
            let mut fields = vec![
                ("message", Field::Str(text.clone())),
                ("error_code", Field::OptStr(error_code)),
            ];
            fields.extend(reply_fields(response.clone()));
            raise!(
                MwaAsvoApiErrorAuthenticationFailed,
                "AuthenticationFailed",
                fields
            )
        }
        lib::MwaAsvoApiError::InvalidParameter {
            name,
            message: text,
        } => raise!(
            MwaAsvoApiErrorInvalidParameter,
            "InvalidParameter",
            vec![
                ("name", Field::Str((*name).to_string())),
                ("message", Field::Str(text.clone())),
            ]
        ),
        lib::MwaAsvoApiError::Conversion(_) => {
            raise!(MwaAsvoApiErrorConversion, "Conversion", vec![])
        }
        lib::MwaAsvoApiError::BadJson(_) => raise!(MwaAsvoApiErrorBadJson, "BadJson", vec![]),
        lib::MwaAsvoApiError::Reqwest(_) => raise!(MwaAsvoApiErrorReqwest, "Reqwest", vec![]),
        lib::MwaAsvoApiError::ApiError { status, response } => {
            let mut fields = vec![
                ("status", Field::Int(u64::from(*status))),
                ("error_code", Field::Str(response.error_code.clone())),
                ("message", Field::Str(response.message.clone())),
            ];
            fields.extend(reply_fields(Some(response.clone())));
            raise!(MwaAsvoApiErrorApiError, "ApiError", fields)
        }
        lib::MwaAsvoApiError::BadStatus {
            status,
            message: text,
        } => raise!(
            MwaAsvoApiErrorBadStatus,
            "BadStatus",
            vec![
                ("status", Field::Int(u64::from(*status))),
                ("message", Field::Str(text.clone())),
            ]
        ),
    }
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

/// Convert a library [`lib::MwaAsvoError`] to a Python exception.
///
/// `lib::MwaAsvoError::MwaAsvoApi` wraps an API error (for example, the job list
/// request before a download), so it becomes `MwaAsvoErrorMwaAsvoApi`, which has
/// the Python `MwaAsvoApiError` as its `source` and its `__cause__`.
/// `lib::MwaAsvoError::IO` and `lib::MwaAsvoError::CannotCreate` (a file or a
/// directory that cannot be read, written or made) become an `OSError`, as
/// Python's `open` raises it: a `FileNotFoundError`, a `PermissionError` and
/// so on. `CannotCreate` has the path as the `filename`.
pub(crate) fn mwa_asvo_error(py: Python<'_>, e: lib::MwaAsvoError) -> PyErr {
    let message = e.to_string();
    let job_id = |id: crate::MwaAsvoJobId| ("job_id", Field::Int(id.get()));
    let obs_id = |o: crate::ObsId| ("obs_id", Field::ObsId(o));
    // The exception of a variant, with `kind` and `fields`.
    macro_rules! raise {
        ($class:ident, $kind:literal, $fields:expr) => {
            build::<$class>(py, message, $kind, $fields)
        };
    }
    match e {
        lib::MwaAsvoError::MwaAsvoApi(inner) => {
            let source = api_error(py, inner);
            let cause = source.value(py).clone().unbind();
            let err = raise!(
                MwaAsvoErrorMwaAsvoApi,
                "MwaAsvoApi",
                vec![("source", Field::Exception(cause.clone_ref(py)))]
            );
            err.set_cause(py, Some(source));
            err
        }
        lib::MwaAsvoError::InvalidOption {
            name,
            message: text,
        } => raise!(
            MwaAsvoErrorInvalidOption,
            "InvalidOption",
            vec![
                ("name", Field::Str(name.to_string())),
                ("message", Field::Str(text)),
            ]
        ),
        lib::MwaAsvoError::IO(source) => PyErr::from(source),
        lib::MwaAsvoError::CannotCreate { path, source } => os_error(py, &path, source),
        lib::MwaAsvoError::NoMwaAsvoJob(id) => {
            raise!(MwaAsvoErrorNoMwaAsvoJob, "NoMwaAsvoJob", vec![job_id(id)])
        }
        lib::MwaAsvoError::JobFailed {
            job_id: id,
            obs_id: o,
            error,
            error_code,
        } => raise!(
            MwaAsvoErrorJobFailed,
            "JobFailed",
            vec![
                job_id(id),
                obs_id(o),
                ("error", Field::Str(error)),
                ("error_code", Field::OptInt(error_code)),
            ]
        ),
        lib::MwaAsvoError::InvalidJob { id, problem } => raise!(
            MwaAsvoErrorInvalidJob,
            "InvalidJob",
            vec![
                ("id", Field::OptInt(i64::try_from(id.get()).ok())),
                ("problem", Field::Str(problem)),
            ]
        ),
        lib::MwaAsvoError::JobCancelled(id) => {
            raise!(MwaAsvoErrorJobCancelled, "JobCancelled", vec![job_id(id)])
        }
        lib::MwaAsvoError::NoObsId(o) => raise!(MwaAsvoErrorNoObsId, "NoObsId", vec![obs_id(o)]),
        lib::MwaAsvoError::NoJobReadyForObsId(o) => raise!(
            MwaAsvoErrorNoJobReadyForObsId,
            "NoJobReadyForObsId",
            vec![obs_id(o)]
        ),
        lib::MwaAsvoError::TooManyObsIds(o) => {
            raise!(MwaAsvoErrorTooManyObsIds, "TooManyObsIds", vec![obs_id(o)])
        }
        lib::MwaAsvoError::NotReady {
            job_id: id,
            job_state,
        } => raise!(
            MwaAsvoErrorNotReady,
            "NotReady",
            vec![
                job_id(id),
                ("job_state", Field::State(PyJobState::from(job_state))),
            ]
        ),
        lib::MwaAsvoError::NoFiles(id) => raise!(MwaAsvoErrorNoFiles, "NoFiles", vec![job_id(id)]),
        lib::MwaAsvoError::JobExpired {
            job_id: id,
            expired,
        } => raise!(
            MwaAsvoErrorJobExpired,
            "JobExpired",
            vec![job_id(id), ("expired", Field::Time(expired))]
        ),
        lib::MwaAsvoError::HashMismatch {
            job_id: id,
            file,
            calculated_hash,
            expected_hash,
        } => raise!(
            MwaAsvoErrorHashMismatch,
            "HashMismatch",
            vec![
                job_id(id),
                ("file", Field::Str(file)),
                ("calculated_hash", Field::Str(calculated_hash)),
                ("expected_hash", Field::Str(expected_hash)),
            ]
        ),
        lib::MwaAsvoError::InvalidJobState { str } => raise!(
            MwaAsvoErrorInvalidJobState,
            "InvalidJobState",
            vec![("str", Field::Str(str))]
        ),
        lib::MwaAsvoError::InvalidJobType { str } => raise!(
            MwaAsvoErrorInvalidJobType,
            "InvalidJobType",
            vec![("str", Field::Str(str))]
        ),
        lib::MwaAsvoError::InvalidEnvironment {
            name,
            value,
            problem,
        } => raise!(
            MwaAsvoErrorInvalidEnvironment,
            "InvalidEnvironment",
            vec![
                ("name", Field::Str(name)),
                ("value", Field::Str(value)),
                ("problem", Field::Str(problem)),
            ]
        ),
        lib::MwaAsvoError::Interrupted => raise!(MwaAsvoErrorInterrupted, "Interrupted", vec![]),
        lib::MwaAsvoError::NoUrl { job_id: id } => {
            raise!(MwaAsvoErrorNoUrl, "NoUrl", vec![job_id(id)])
        }
        lib::MwaAsvoError::NoPath { job_id: id } => {
            raise!(MwaAsvoErrorNoPath, "NoPath", vec![job_id(id)])
        }
        lib::MwaAsvoError::NoHash { job_id: id } => {
            raise!(MwaAsvoErrorNoHash, "NoHash", vec![job_id(id)])
        }
        lib::MwaAsvoError::FilesNotReachable {
            job_id: id,
            delivery,
            path,
        } => raise!(
            MwaAsvoErrorFilesNotReachable,
            "FilesNotReachable",
            vec![
                job_id(id),
                (
                    "delivery",
                    Field::Delivery(PyDeliveryDestination::from(delivery))
                ),
                ("path", Field::OptStr(path)),
            ]
        ),
        lib::MwaAsvoError::SymlinkInDownloadDir { path } => raise!(
            MwaAsvoErrorSymlinkInDownloadDir,
            "SymlinkInDownloadDir",
            vec![("file", Field::Str(path.display().to_string()))]
        ),
        lib::MwaAsvoError::DestinationExists { job_id: id, path } => raise!(
            MwaAsvoErrorDestinationExists,
            "DestinationExists",
            vec![job_id(id), ("file", Field::Str(path.display().to_string()))]
        ),
        lib::MwaAsvoError::HttpError {
            job_id: id,
            status,
            message,
        } => raise!(
            MwaAsvoErrorHttpError,
            "HttpError",
            vec![
                job_id(id),
                ("status", Field::Int(u64::from(status))),
                ("message", Field::Str(message)),
            ]
        ),
        lib::MwaAsvoError::Http404Error { job_id: id } => {
            raise!(MwaAsvoErrorHttp404Error, "Http404Error", vec![job_id(id)])
        }
    }
}

/// Convert a library [`LibObsIdError`] to a Python `ObsIdError`: the class of
/// its variant.
pub(crate) fn obs_id_error(py: Python<'_>, e: LibObsIdError) -> PyErr {
    let message = e.to_string();
    match e {
        LibObsIdError::WrongNumDigits(value) => build_from(
            py,
            PyErr::new::<ObsIdErrorWrongNumDigits, _>(message),
            "WrongNumDigits",
            vec![("value", Field::Int(value))],
        ),
        LibObsIdError::Parse(_) => build_from(
            py,
            PyErr::new::<ObsIdErrorParse, _>(message),
            "Parse",
            vec![],
        ),
    }
}

/// Convert a library [`LibParseError`] to a Python `ParseError`: the class of
/// its variant. A file or standard input that cannot be read is an `OSError`.
pub(crate) fn parse_error(py: Python<'_>, e: LibParseError) -> PyErr {
    let message = e.to_string();
    // The exception of a variant, with `kind` and `fields`.
    macro_rules! raise {
        ($class:ident, $kind:literal, $fields:expr) => {
            build_from(py, PyErr::new::<$class, _>(message), $kind, $fields)
        };
    }
    match e {
        LibParseError::InsideFile { file, text } => raise!(
            ParseErrorInsideFile,
            "InsideFile",
            vec![("file", Field::Str(file)), ("text", Field::Str(text))]
        ),
        LibParseError::InsideStdin { text } => {
            raise!(
                ParseErrorInsideStdin,
                "InsideStdin",
                vec![("text", Field::Str(text))]
            )
        }
        LibParseError::StdinIO { source } => PyErr::from(source),
        LibParseError::StdinError { message: problem } => raise!(
            ParseErrorStdinError,
            "StdinError",
            vec![("message", Field::Str(problem))]
        ),
        LibParseError::StdinJson { problem } => raise!(
            ParseErrorStdinJson,
            "StdinJson",
            vec![("problem", Field::Str(problem))]
        ),
        LibParseError::IO { file, source } => os_error(py, &file, source),
        LibParseError::JobIdsGiven { job_ids } => raise!(
            ParseErrorJobIdsGiven,
            "JobIdsGiven",
            vec![(
                "job_ids",
                Field::Ints(job_ids.into_iter().map(|id| id.get()).collect())
            )]
        ),
        LibParseError::ObsIdsGiven { obs_ids } => raise!(
            ParseErrorObsIdsGiven,
            "ObsIdsGiven",
            vec![("obs_ids", Field::ObsIds(obs_ids))]
        ),
        LibParseError::ZeroJobId => raise!(ParseErrorZeroJobId, "ZeroJobId", vec![]),
        LibParseError::InvalidObsId { value } => raise!(
            ParseErrorInvalidObsId,
            "InvalidObsId",
            vec![("value", Field::Int(value))]
        ),
        LibParseError::NoObsIds => raise!(ParseErrorNoObsIds, "NoObsIds", vec![]),
        LibParseError::NoJobIds => raise!(ParseErrorNoJobIds, "NoJobIds", vec![]),
        LibParseError::InvalidTime => raise!(ParseErrorInvalidTime, "InvalidTime", vec![]),
    }
}
