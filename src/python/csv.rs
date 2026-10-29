// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python side of the import of a `manta-ray-client` CSV file: the
//! library's `CsvDefaults`, `CsvJob`, `CsvJobParams`, `CsvRowError`,
//! `CsvJobFailure` and its errors, and the functions that read a file.
//!
//! The Python methods `AsvoClient.submit_csv_file`, `submit_csv_jobs` and
//! `submit_csv_job` are the library's methods of the same names (see
//! `super::client`).
//!
//! The errors are the library's, with a Python class for each kind, as in
//! `super::error`:
//!
//! - [`CsvError`] (a file that has problems on some lines) has the
//!   `errors` of its lines, each a [`PyCsvRowError`].
//! - [`CsvFileError`] (`read_manta_ray_csv`) and [`CsvSubmitError`]
//!   (`submit_csv_file`) have a subclass for each kind: `Invalid` (with
//!   `errors`), `NoJobs` (with `path`) and, for the submit, `Failed` (with
//!   `submitted` and `failures`). A file that cannot be read (`Read`) is an
//!   `OSError`, as in the rest of the module.
//! - [`CsvJobsFailed`] (`submit_csv_jobs`) has `submitted` and `failures`.

use std::path::PathBuf;

use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use super::error::{api_error, api_error_ref, os_error, variant_exceptions, KIND};
use super::job_params::{PyConversionJobParams, PyDownloadJobParams, PyImagingJobFlow1Params};
use super::obs_id::{ObsIdArg, PyObsId};
use super::typed::ExceptionObject;
use super::types::{PyDeliveryDestination, PyDeliveryFormat, PyJobSubmittedResponse};
use crate::obs_id::ObsId;
use crate::{
    CsvDefaults, CsvError as LibCsvError, CsvFileError as LibCsvFileError, CsvJob, CsvJobFailure,
    CsvJobParams as LibCsvJobParams, CsvJobsFailed as LibCsvJobsFailed,
    CsvRowError as LibCsvRowError, CsvSubmitError as LibCsvSubmitError,
};

// With the "python-stubgen" feature, pyo3-stub-gen's `create_exception!`
// wraps pyo3's and also registers the exception for the stubs.
#[cfg(not(feature = "python-stubgen"))]
use pyo3::create_exception;
#[cfg(feature = "python-stubgen")]
use pyo3_stub_gen::create_exception;

create_exception!(
    mwa_giant_squid,
    CsvError,
    PyException,
    "A CSV file has a problem on some lines. `errors` has a `CsvRowError` for each of them, in the order of the lines. There is at least one."
);

create_exception!(
    mwa_giant_squid,
    CsvJobsFailed,
    PyException,
    "`AsvoClient.submit_csv_jobs` submitted the jobs, but some jobs failed.\n\nThe jobs that did not fail were submitted: a failed job does not stop the others. `submitted` has the replies of the jobs that were submitted, and `failures` has one `CsvJobFailure` for each job that was not, both in the order of the file."
);

create_exception!(
    mwa_giant_squid,
    CsvFileError,
    PyException,
    "`read_manta_ray_csv` could not turn a CSV file into jobs. `kind` is the Rust `CsvFileError` variant, and the exception is also an instance of the class for that variant (for example `CsvFileErrorNoJobs`). A file that cannot be read is an `OSError`, not this.\n\n`kind` is always set. The other attributes are set only for the kinds shown."
);

create_exception!(
    mwa_giant_squid,
    CsvSubmitError,
    PyException,
    "`AsvoClient.submit_csv_file` did not submit every job of a file. `kind` is the Rust `CsvSubmitError` variant, and the exception is also an instance of the class for that variant (for example `CsvSubmitErrorFailed`). A file that cannot be read is an `OSError`, not this.\n\n`kind` is always set. The other attributes are set only for the kinds shown. For every kind but `Failed`, nothing was submitted. For `Failed`, the jobs that did not fail were submitted: a failed job does not stop the others."
);

variant_exceptions! {
    CsvFileError:
    CsvFileErrorInvalid => "A line of the file has a problem (kind `Invalid`). `errors` has a `CsvRowError` for each of them.",
    CsvFileErrorNoJobs => "The file has no job: it is empty, or has only blank lines and comments (kind `NoJobs`). `path` is the file.",
}

variant_exceptions! {
    CsvSubmitError:
    CsvSubmitErrorInvalid => "A line of the file has a problem. Nothing was submitted (kind `Invalid`). `errors` has a `CsvRowError` for each of them.",
    CsvSubmitErrorNoJobs => "The file has no job: it is empty, or has only blank lines and comments. Nothing was submitted (kind `NoJobs`). `path` is the file.",
    CsvSubmitErrorFailed => "The server refused some jobs, or a request failed (kind `Failed`). The other jobs were submitted: a failed job does not stop the others. `submitted` has the replies of the jobs that were submitted, and `failures` has one `CsvJobFailure` for each job that was not, both in the order of the file.",
}

/// The attributes of the exceptions, for the stubs. The functions below set
/// them at run time, so pyo3-stub-gen cannot see them.
#[cfg(feature = "python-stubgen")]
mod stub_attributes {
    use pyo3_stub_gen::type_info::{MemberInfo, PyMethodsInfo};
    use pyo3_stub_gen::PyStubType;

    use super::{
        CsvError, CsvFileError, CsvJobsFailed, CsvSubmitError, PyCsvJobFailure, PyCsvRowError,
        PyJobSubmittedResponse,
    };

    /// A `MemberInfo` of an attribute of an exception.
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

    /// The `PyMethodsInfo` of an exception that has these attributes.
    macro_rules! exception_attrs {
        ($exception:ty, [$($attr:expr),* $(,)?]) => {
            pyo3_stub_gen::inventory::submit! {
                PyMethodsInfo {
                    struct_id: std::any::TypeId::of::<$exception>,
                    attrs: &[$($attr),*],
                    getters: &[],
                    setters: &[],
                    methods: &[],
                    file: file!(),
                    line: line!(),
                    column: column!(),
                }
            }
        };
    }

    exception_attrs!(
        CsvError,
        [attr!(
            "errors",
            Vec<PyCsvRowError>,
            "The lines that have a problem, in the order of the lines."
        )]
    );
    exception_attrs!(
        CsvJobsFailed,
        [
            attr!(
                "submitted",
                Vec<PyJobSubmittedResponse>,
                "The replies for the jobs that were submitted, in the order of the file."
            ),
            attr!(
                "failures",
                Vec<PyCsvJobFailure>,
                "The jobs that were not submitted, in the order of the file."
            ),
        ]
    );
    exception_attrs!(
        CsvFileError,
        [
            attr!("kind", String, "\"Invalid\" or \"NoJobs\"."),
            attr!(
                "errors",
                Vec<PyCsvRowError>,
                "Invalid: the lines that have a problem."
            ),
            attr!("path", std::path::PathBuf, "NoJobs: the file."),
        ]
    );
    exception_attrs!(
        CsvSubmitError,
        [
            attr!("kind", String, "\"Invalid\", \"NoJobs\" or \"Failed\"."),
            attr!(
                "errors",
                Vec<PyCsvRowError>,
                "Invalid: the lines that have a problem."
            ),
            attr!("path", std::path::PathBuf, "NoJobs: the file."),
            attr!(
                "submitted",
                Vec<PyJobSubmittedResponse>,
                "Failed: the replies for the jobs that were submitted, in the order of the file."
            ),
            attr!(
                "failures",
                Vec<PyCsvJobFailure>,
                "Failed: the jobs that were not submitted, in the order of the file."
            ),
        ]
    );
}

/// What is wrong with one line of a CSV file.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "CsvRowError",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyCsvRowError {
    line: usize,
    message: String,
}

impl From<&LibCsvRowError> for PyCsvRowError {
    fn from(error: &LibCsvRowError) -> Self {
        Self {
            line: error.line,
            message: error.message.clone(),
        }
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyCsvRowError {
    /// The line of the file, from 1.
    #[getter]
    fn line(&self) -> usize {
        self.line
    }

    /// What is wrong, in words.
    #[getter]
    fn message(&self) -> String {
        self.message.clone()
    }

    fn __str__(&self) -> String {
        format!("line {}: {}", self.line, self.message)
    }

    fn __repr__(&self) -> String {
        format!(
            "CsvRowError(line={}, message={:?})",
            self.line, self.message
        )
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
    obs_id: PyObsId,
    error: Py<PyAny>,
}

impl PyCsvJobFailure {
    fn new(py: Python<'_>, failure: CsvJobFailure) -> Self {
        let error = api_error(py, failure.error);
        Self {
            line: failure.line,
            obs_id: PyObsId(failure.obs_id),
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
    fn obs_id(&self) -> PyObsId {
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
            self.line, self.obs_id.0
        )
    }
}

/// What the lines of a CSV file do not give, for the values that the
/// environment or a command line can set for a whole file.
///
/// A key that a line gives wins. Every other key that a line does not give has
/// the default of the MWA ASVO.
///
/// Args:
///     delivery: The `delivery` of a line that has none. `None` leaves the
///         default of the MWA ASVO.
///     delivery_format: The `delivery_format` of a line that has none.
///         `None` leaves the default of the MWA ASVO.
///     allow_resubmit: The `allow_resubmit` of a line that has none. `None`
///         leaves the default of the MWA ASVO.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(skip_from_py_object, name = "CsvDefaults", module = "mwa_giant_squid")]
#[derive(Clone)]
pub struct PyCsvDefaults {
    /// The `delivery` of a line that has none.
    #[pyo3(get, set)]
    pub delivery: Option<PyDeliveryDestination>,
    /// The `delivery_format` of a line that has none.
    #[pyo3(get, set)]
    pub delivery_format: Option<PyDeliveryFormat>,
    /// The `allow_resubmit` of a line that has none.
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyCsvDefaults {
    #[new]
    #[pyo3(signature = (*, delivery=None, delivery_format=None, allow_resubmit=None))]
    fn new(
        delivery: Option<PyDeliveryDestination>,
        delivery_format: Option<PyDeliveryFormat>,
        allow_resubmit: Option<bool>,
    ) -> Self {
        Self {
            delivery,
            delivery_format,
            allow_resubmit,
        }
    }

    /// The defaults that come from the environment, as for the `submit-*`
    /// commands.
    ///
    /// - `GIANT_SQUID_DELIVERY` is the `delivery` of a line that has none.
    /// - `GIANT_SQUID_DELIVERY_FORMAT` is the `delivery_format` of a line
    ///   that has none.
    ///
    /// `allow_resubmit` is not read from the environment.
    ///
    /// Returns:
    ///     The defaults.
    ///
    /// Raises:
    ///     AsvoError: A variable is set to a value that the schema does not
    ///         list (kind `InvalidEnvironment`).
    #[staticmethod]
    fn from_env(py: Python<'_>) -> PyResult<Self> {
        CsvDefaults::from_env()
            .map(Self::from)
            .map_err(|e| super::error::asvo_error(py, e))
    }

    fn __repr__(&self) -> String {
        let delivery = self
            .delivery
            .map(|d| crate::mwa_asvo::api::openapi::DeliveryDestination::from(d).to_string());
        let format = self
            .delivery_format
            .map(|f| crate::mwa_asvo::api::openapi::DeliveryFormat::from(f).to_string());
        format!(
            "CsvDefaults(delivery={delivery:?}, delivery_format={format:?}, allow_resubmit={:?})",
            self.allow_resubmit
        )
    }
}

impl From<CsvDefaults> for PyCsvDefaults {
    fn from(defaults: CsvDefaults) -> Self {
        Self {
            delivery: defaults.delivery.map(Into::into),
            delivery_format: defaults.delivery_format.map(Into::into),
            allow_resubmit: defaults.allow_resubmit,
        }
    }
}

impl PyCsvDefaults {
    /// The library's defaults for these values.
    pub(super) fn to_lib(&self) -> CsvDefaults {
        CsvDefaults {
            delivery: self.delivery.map(Into::into),
            delivery_format: self.delivery_format.map(Into::into),
            allow_resubmit: self.allow_resubmit,
        }
    }

    /// The library's defaults for an argument that is `None` or a
    /// `CsvDefaults`.
    pub(super) fn lib_or_default(defaults: Option<PyRef<'_, Self>>) -> CsvDefaults {
        defaults.map_or_else(CsvDefaults::default, |d| d.to_lib())
    }
}

/// The request body of one job of a CSV file: the library's `CsvJobParams`.
///
/// `CsvJobParams.Conversion(params)` (`job_type=c`), `CsvJobParams.Imaging(params)`
/// (`job_type=i`), `CsvJobParams.Download(params)` (`job_type=d`, a visibility
/// or metadata download), `CsvJobParams.Voltage(params)` (`job_type=v`) and
/// `CsvJobParams.Beamformer(params)` (`job_type=b`) hold the request body.
/// `Voltage` and `Beamformer` hold a `DownloadJobParams`, as `Download` does.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "CsvJobParams",
    module = "mwa_giant_squid"
)]
pub enum PyCsvJobParams {
    /// A conversion job (`job_type=c`).
    Conversion {
        /// The request body.
        params: Py<PyConversionJobParams>,
    },
    /// An imaging job from an Obs ID (`job_type=i`).
    Imaging {
        /// The request body.
        params: Py<PyImagingJobFlow1Params>,
    },
    /// A visibility or metadata download job (`job_type=d`).
    Download {
        /// The request body.
        params: Py<PyDownloadJobParams>,
    },
    /// A voltage job (`job_type=v`): a download job with `offset` and
    /// `duration`.
    Voltage {
        /// The request body.
        params: Py<PyDownloadJobParams>,
    },
    /// A beamformer job (`job_type=b`): a download job. The MWA ASVO finds
    /// the kind of download from the observation.
    Beamformer {
        /// The request body.
        params: Py<PyDownloadJobParams>,
    },
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyCsvJobParams {
    /// The kind of job, in words: for example `conversion`.
    ///
    /// Raises:
    ///     ValueError: The image size or `nmiter` of an imaging job is not
    ///         valid.
    fn description(&self, py: Python<'_>) -> PyResult<&'static str> {
        Ok(self.to_lib(py)?.description())
    }

    /// Check the request body against the limits of the schema, as the
    /// client does before it sends the request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        self.to_lib(py)?.validate().map_err(|e| api_error(py, e))
    }
}

impl PyCsvJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ValueError` if the image size or `nmiter` is not valid.
    pub(super) fn to_lib(&self, py: Python<'_>) -> PyResult<LibCsvJobParams> {
        Ok(match self {
            Self::Conversion { params } => {
                LibCsvJobParams::Conversion(Box::new(params.borrow(py).to_schema()))
            }
            Self::Imaging { params } => {
                LibCsvJobParams::Imaging(Box::new(params.borrow(py).to_schema(py)?))
            }
            Self::Download { params } => LibCsvJobParams::Download(params.borrow(py).to_schema()),
            Self::Voltage { params } => LibCsvJobParams::Voltage(params.borrow(py).to_schema()),
            Self::Beamformer { params } => {
                LibCsvJobParams::Beamformer(params.borrow(py).to_schema())
            }
        })
    }

    /// The Python request body for the library's, which is for `obs_id`.
    fn from_lib(py: Python<'_>, obs_id: ObsId, params: LibCsvJobParams) -> PyResult<Self> {
        Ok(match params {
            LibCsvJobParams::Conversion(p) => Self::Conversion {
                params: Py::new(py, PyConversionJobParams::from_schema(obs_id, *p))?,
            },
            LibCsvJobParams::Imaging(p) => Self::Imaging {
                params: Py::new(py, PyImagingJobFlow1Params::from_schema(obs_id, *p))?,
            },
            LibCsvJobParams::Download(p) => Self::Download {
                params: Py::new(py, PyDownloadJobParams::from_schema(obs_id, p))?,
            },
            LibCsvJobParams::Voltage(p) => Self::Voltage {
                params: Py::new(py, PyDownloadJobParams::from_schema(obs_id, p))?,
            },
            LibCsvJobParams::Beamformer(p) => Self::Beamformer {
                params: Py::new(py, PyDownloadJobParams::from_schema(obs_id, p))?,
            },
        })
    }
}

/// One job of a CSV file.
///
/// Args:
///     line: The line of the file, from 1.
///     obs_id: The Obs ID of the job.
///     params: The request body.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "CsvJob",
    module = "mwa_giant_squid"
)]
pub struct PyCsvJob {
    line: usize,
    obs_id: ObsId,
    params: Py<PyCsvJobParams>,
}

impl PyCsvJob {
    /// The Python job for the library's.
    fn from_lib(py: Python<'_>, job: CsvJob) -> PyResult<Self> {
        // `into_pyobject` makes the instance of the variant's class, as Python
        // matches it (`Py::new` would make the base class).
        let params = PyCsvJobParams::from_lib(py, job.obs_id, job.params)?
            .into_pyobject(py)?
            .cast_into::<PyCsvJobParams>()?
            .unbind();
        Ok(Self {
            line: job.line,
            obs_id: job.obs_id,
            params,
        })
    }

    /// The library's job for this one.
    ///
    /// # Errors
    ///
    /// `ValueError` if the image size or `nmiter` of an imaging job is not
    /// valid.
    pub(super) fn to_lib(&self, py: Python<'_>) -> PyResult<CsvJob> {
        Ok(CsvJob {
            line: self.line,
            obs_id: self.obs_id,
            params: self.params.get().to_lib(py)?,
        })
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyCsvJob {
    #[new]
    fn new(line: usize, obs_id: ObsIdArg, params: Py<PyCsvJobParams>) -> Self {
        Self {
            line,
            obs_id: obs_id.0,
            params,
        }
    }

    /// The line of the file, from 1.
    #[getter]
    fn line(&self) -> usize {
        self.line
    }

    /// The Obs ID of the job.
    #[getter]
    fn obs_id(&self) -> PyObsId {
        PyObsId(self.obs_id)
    }

    /// The request body.
    #[getter]
    fn params(&self, py: Python<'_>) -> Py<PyCsvJobParams> {
        self.params.clone_ref(py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let description = self
            .params
            .get()
            .to_lib(py)
            .map_or("job", |p| p.description());
        format!(
            "CsvJob(line={}, obs_id={}, {description})",
            self.line, self.obs_id
        )
    }
}

/// The problems of a file as a list of `CsvRowError`.
fn row_errors(error: &LibCsvError) -> Vec<PyCsvRowError> {
    error.errors.iter().map(PyCsvRowError::from).collect()
}

/// Set `kind` (if there is one) and the attributes of the exception `err`
/// with `set`, and return it.
fn with_attributes(
    py: Python<'_>,
    err: PyErr,
    kind: Option<&str>,
    set: impl FnOnce(&Bound<'_, pyo3::exceptions::PyBaseException>) -> PyResult<()>,
) -> PyErr {
    let value = err.value(py);
    let result = match kind {
        Some(kind) => value.setattr(KIND, kind),
        None => Ok(()),
    }
    .and_then(|()| set(value));
    // Setting an attribute on a new exception instance does not fail in
    // practice; if it ever does, raise that error instead.
    match result {
        Ok(()) => err,
        Err(e) => e,
    }
}

/// Convert a library [`LibCsvError`] to a Python `CsvError`.
pub(crate) fn csv_error(py: Python<'_>, e: LibCsvError) -> PyErr {
    let err = PyErr::new::<CsvError, _>(e.to_string());
    with_attributes(py, err, None, |value| {
        value.setattr("errors", row_errors(&e))
    })
}

/// Convert a library [`LibCsvFileError`] to a Python exception: an `OSError`
/// for a file that cannot be read, and a [`CsvFileError`] subclass otherwise.
pub(crate) fn csv_file_error(py: Python<'_>, e: LibCsvFileError) -> PyErr {
    let message = e.to_string();
    match e {
        LibCsvFileError::Read { path, source } => os_error(py, &path, source),
        LibCsvFileError::Invalid(error) => with_attributes(
            py,
            PyErr::new::<CsvFileErrorInvalid, _>(message),
            Some("Invalid"),
            |value| value.setattr("errors", row_errors(&error)),
        ),
        LibCsvFileError::NoJobs { path } => with_attributes(
            py,
            PyErr::new::<CsvFileErrorNoJobs, _>(message),
            Some("NoJobs"),
            |value| value.setattr("path", path),
        ),
    }
}

/// The replies and the failures of jobs, for the exceptions that have them.
fn submitted_and_failures(
    py: Python<'_>,
    submitted: Vec<crate::mwa_asvo::api::openapi::JobSubmittedResponse>,
    failures: Vec<CsvJobFailure>,
) -> (Vec<PyJobSubmittedResponse>, Vec<PyCsvJobFailure>) {
    (
        submitted.into_iter().map(Into::into).collect(),
        failures
            .into_iter()
            .map(|failure| PyCsvJobFailure::new(py, failure))
            .collect(),
    )
}

/// Convert a library [`LibCsvJobsFailed`] to a Python `CsvJobsFailed`.
pub(crate) fn csv_jobs_failed(py: Python<'_>, e: LibCsvJobsFailed) -> PyErr {
    let message = e.to_string();
    let (submitted, failures) = submitted_and_failures(py, e.submitted, e.failures);
    with_attributes(py, PyErr::new::<CsvJobsFailed, _>(message), None, |value| {
        value.setattr("submitted", submitted)?;
        value.setattr("failures", failures)
    })
}

/// Convert a library [`LibCsvSubmitError`] to a Python exception: an `OSError`
/// for a file that cannot be read, and a [`CsvSubmitError`] subclass
/// otherwise.
pub(crate) fn csv_submit_error(py: Python<'_>, e: LibCsvSubmitError) -> PyErr {
    let message = e.to_string();
    match e {
        LibCsvSubmitError::Read { path, source } => os_error(py, &path, source),
        LibCsvSubmitError::Invalid(error) => with_attributes(
            py,
            PyErr::new::<CsvSubmitErrorInvalid, _>(message),
            Some("Invalid"),
            |value| value.setattr("errors", row_errors(&error)),
        ),
        LibCsvSubmitError::NoJobs { path } => with_attributes(
            py,
            PyErr::new::<CsvSubmitErrorNoJobs, _>(message),
            Some("NoJobs"),
            |value| value.setattr("path", path),
        ),
        LibCsvSubmitError::Failed {
            submitted,
            failures,
        } => {
            let (submitted, failures) = submitted_and_failures(py, submitted, failures);
            with_attributes(
                py,
                PyErr::new::<CsvSubmitErrorFailed, _>(message),
                Some("Failed"),
                |value| {
                    value.setattr("submitted", submitted)?;
                    value.setattr("failures", failures)
                },
            )
        }
    }
}

/// The Python `CsvJob` of each library job.
fn python_jobs(py: Python<'_>, jobs: Vec<CsvJob>) -> PyResult<Vec<PyCsvJob>> {
    jobs.into_iter()
        .map(|job| PyCsvJob::from_lib(py, job))
        .collect()
}

/// Read the jobs of the text of a `manta-ray-client` CSV file.
///
/// Every line is read and checked (also against the limits of the schema)
/// before the function returns, so that a file with a bad line at its end
/// submits nothing. The jobs are in the order of the lines. A file with no job
/// gives an empty list.
///
/// A key that a line gives twice (the last value is used), and a line that
/// makes the same job as an earlier line, are logged as warnings.
///
/// Args:
///     text: The text of the file.
///     defaults: What the lines do not give (a `CsvDefaults`). `None` is no
///         defaults.
///
/// Returns:
///     The jobs, in the order of the lines.
///
/// Raises:
///     CsvError: Some lines have a problem. `errors` has all of them.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
#[pyo3(signature = (text, defaults=None))]
pub(super) fn parse_manta_ray_csv(
    py: Python<'_>,
    text: &str,
    defaults: Option<PyRef<'_, PyCsvDefaults>>,
) -> PyResult<Vec<PyCsvJob>> {
    let defaults = PyCsvDefaults::lib_or_default(defaults);
    let jobs = py
        .detach(|| crate::parse_manta_ray_csv(text, &defaults))
        .map_err(|e| csv_error(py, e))?;
    python_jobs(py, jobs)
}

/// Read the jobs of a `manta-ray-client` CSV file: read the file and parse it
/// as `parse_manta_ray_csv` does, which checks every line.
///
/// Args:
///     path: The file.
///     defaults: What the lines do not give (a `CsvDefaults`). `None` is no
///         defaults.
///
/// Returns:
///     The jobs, in the order of the lines.
///
/// Raises:
///     OSError: The file cannot be read.
///     CsvFileError: Some lines have a problem (kind `Invalid`, with
///         `errors`), or the file has no job (kind `NoJobs`: a command that
///         was asked to submit a file with nothing in it has been used
///         wrongly).
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
#[pyo3(signature = (path, defaults=None))]
pub(super) fn read_manta_ray_csv(
    py: Python<'_>,
    path: PathBuf,
    defaults: Option<PyRef<'_, PyCsvDefaults>>,
) -> PyResult<Vec<PyCsvJob>> {
    let defaults = PyCsvDefaults::lib_or_default(defaults);
    let jobs = py
        .detach(|| crate::read_manta_ray_csv(&path, &defaults))
        .map_err(|e| csv_file_error(py, e))?;
    python_jobs(py, jobs)
}

/// The result of one job for the `on_result` function of `submit_csv_jobs`:
/// the reply, or the `AsvoApiError` of the failed request.
pub(super) fn result_object<'py>(
    py: Python<'py>,
    result: &Result<
        crate::mwa_asvo::api::openapi::JobSubmittedResponse,
        crate::mwa_asvo::AsvoApiError,
    >,
) -> PyResult<Bound<'py, PyAny>> {
    match result {
        Ok(response) => Ok(PyJobSubmittedResponse::from(response.clone())
            .into_pyobject(py)?
            .into_any()),
        Err(error) => Ok(api_error_ref(py, error)
            .into_value(py)
            .into_bound(py)
            .into_any()),
    }
}
