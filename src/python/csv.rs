// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python parts of the import of a manta-ray-client CSV file: the
//! exception `CsvSubmitError` and the class `CsvJobFailure`.
//!
//! The Python method `AsvoClient.submit_csv_file` is the library's
//! [`AsvoClient::submit_csv_file`](crate::AsvoClient::submit_csv_file). It
//! returns the replies in a list. A problem with the file is a `ValueError`
//! and a file that cannot be read is an `OSError`, as in the rest of the
//! module: a bad argument is a `ValueError`. A job that fails is a
//! `CsvSubmitError`, which has the replies of the jobs that were submitted
//! and a `CsvJobFailure` for each job that was not.

use std::path::Path;

use pyo3::exceptions::{PyException, PyOSError, PyValueError};
use pyo3::prelude::*;

use super::error::api_error;
use super::typed::ExceptionObject;
use super::types::PyJobSubmittedResponse;
// The library's error has the name of the Python exception.
use crate::{CsvJobFailure, CsvSubmitError as LibCsvSubmitError};

// With the "python-stubgen" feature, pyo3-stub-gen's `create_exception!`
// wraps pyo3's and also registers the exception for the stubs.
#[cfg(not(feature = "python-stubgen"))]
use pyo3::create_exception;
#[cfg(feature = "python-stubgen")]
use pyo3_stub_gen::create_exception;

create_exception!(
    mwa_giant_squid,
    CsvSubmitError,
    PyException,
    "`AsvoClient.submit_csv_file` submitted the jobs of a file, but some jobs failed.\n\nThe jobs that did not fail were submitted: a failed job does not stop the others. `submitted` has the replies of the jobs that were submitted, and `failures` has one `CsvJobFailure` for each job that was not, both in the order of the file."
);

/// The attributes of the exception, for the stubs. `csv_submit_error` sets
/// them at run time, so pyo3-stub-gen cannot see them.
#[cfg(feature = "python-stubgen")]
mod stub_attributes {
    use pyo3_stub_gen::type_info::{MemberInfo, PyMethodsInfo};
    use pyo3_stub_gen::PyStubType;

    use super::super::types::PyJobSubmittedResponse;
    use super::{CsvSubmitError, PyCsvJobFailure};

    pyo3_stub_gen::inventory::submit! {
        PyMethodsInfo {
            struct_id: std::any::TypeId::of::<CsvSubmitError>,
            attrs: &[
                MemberInfo {
                    name: "submitted",
                    r#type: <Vec<PyJobSubmittedResponse> as PyStubType>::type_output,
                    doc: "The replies for the jobs that were submitted, in the order of the file.",
                    default: None,
                    deprecated: None,
                },
                MemberInfo {
                    name: "failures",
                    r#type: <Vec<PyCsvJobFailure> as PyStubType>::type_output,
                    doc: "The jobs that were not submitted, in the order of the file.",
                    default: None,
                    deprecated: None,
                },
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

/// A job of a CSV file that was not submitted.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "CsvJobFailure",
    module = "mwa_giant_squid"
)]
pub struct PyCsvJobFailure {
    line: usize,
    obs_id: u64,
    error: Py<PyAny>,
}

impl PyCsvJobFailure {
    fn new(py: Python<'_>, failure: CsvJobFailure) -> Self {
        let error = api_error(py, failure.error);
        Self {
            line: failure.line,
            obs_id: u64::from(failure.obs_id),
            error: error.value(py).clone().into_any().unbind(),
        }
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyCsvJobFailure {
    /// The line of the file, from 1.
    #[getter]
    fn line(&self) -> usize {
        self.line
    }

    /// The Obs ID of the job.
    #[getter]
    fn obs_id(&self) -> u64 {
        self.obs_id
    }

    /// Why the job was not submitted: the `AsvoApiError` of the request.
    #[getter]
    fn error(&self, py: Python<'_>) -> ExceptionObject {
        ExceptionObject(self.error.clone_ref(py))
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let message = self
            .error
            .bind(py)
            .str()
            .map_or_else(|_| String::new(), |s| s.to_string());
        format!(
            "CsvJobFailure(line={}, obs_id={}, error={message:?})",
            self.line, self.obs_id
        )
    }
}

/// The `OSError` for a file that cannot be read, made as Python's `open`
/// makes it: `OSError(errno, strerror, filename)`, which Python turns into
/// the subclass for the error (`FileNotFoundError`, `PermissionError` and so
/// on). An error with no error number (for example, text that is not UTF-8)
/// is the error that pyo3 makes, with the file name set.
fn os_error(py: Python<'_>, path: &Path, source: std::io::Error) -> PyErr {
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

/// Convert a library [`LibCsvSubmitError`] to a Python exception.
///
/// - A file that cannot be read is an `OSError` (a `FileNotFoundError`, a
///   `PermissionError` and so on, as Python raises for `open`), with the file
///   name set (see [`os_error`]).
/// - A file with a problem on a line is a `ValueError`, as for the other
///   arguments that the schema does not allow. Nothing was sent.
/// - Jobs that failed are a [`CsvSubmitError`], with `submitted` and
///   `failures`.
pub(crate) fn csv_submit_error(py: Python<'_>, e: LibCsvSubmitError) -> PyErr {
    let message = e.to_string();
    match e {
        LibCsvSubmitError::Read { path, source } => os_error(py, &path, source),
        LibCsvSubmitError::Invalid(_) => PyValueError::new_err(message),
        LibCsvSubmitError::Failed {
            submitted,
            failures,
        } => {
            let err = PyErr::new::<CsvSubmitError, _>(message);
            let value = err.value(py);
            let submitted: Vec<PyJobSubmittedResponse> =
                submitted.into_iter().map(Into::into).collect();
            let failures: Vec<PyCsvJobFailure> = failures
                .into_iter()
                .map(|failure| PyCsvJobFailure::new(py, failure))
                .collect();
            match value
                .setattr("submitted", submitted)
                .and_then(|()| value.setattr("failures", failures))
            {
                Ok(()) => err,
                Err(e) => e,
            }
        }
    }
}
