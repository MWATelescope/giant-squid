// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Python classes for the library's job types.
//!
//! Each class wraps the library type and has the same name and field names.
//! Where the library uses a type of the OpenAPI schema (for example
//! `JobState`), the Python class has the schema's name and members.

use jiff::Timestamp;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::error::asvo_error;
use super::typed::{JobId, JobIterator, JsonDict};
use crate::mwa_asvo::api::openapi::{
    Centre, DeliveryFormat, JobCancelledResponse, JobSubmittedResponse, Output, OutputMode,
    Polarization, Status, Type, Weighting,
};
use crate::mwa_asvo::api::schema_enums::for_each_schema_enum;
use crate::mwa_asvo::{
    AsvoJob, AsvoJobId, AsvoJobVec, Delivery, DownloadProgress, JobFile, JobProduct, JobState,
    JobType,
};
use crate::obs_id::ObsId;

/// Define the Python enum of a schema enum: the same members, and
/// conversions in both directions. Called by `for_each_schema_enum!`, which
/// has the list of every schema enum (see
/// [`crate::mwa_asvo::api::schema_enums`]).
macro_rules! py_enum {
    ($lib:ident, $py:ident, $name:literal, $doc:literal, [$($variant:ident),+ $(,)?]) => {
        #[doc = $doc]
        #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass_enum)]
        #[pyclass(eq, eq_int, frozen, hash, from_py_object, name = $name, module = "mwa_giant_squid")]
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum $py {
            $($variant),+
        }

        impl From<$lib> for $py {
            fn from(v: $lib) -> Self {
                match v {
                    $($lib::$variant => $py::$variant),+
                }
            }
        }

        impl From<$py> for $lib {
            fn from(v: $py) -> Self {
                match v {
                    $($py::$variant => $lib::$variant),+
                }
            }
        }

        #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
        #[pymethods]
        impl $py {
            fn __str__(&self) -> String {
                $lib::from(*self).to_string()
            }
        }
    };
}

// One Python enum for each schema enum (`Delivery`, `JobState` and the others).
for_each_schema_enum!(py_enum);

/// The type of an MWA ASVO job: the OpenAPI schema's `JobType`, whose value
/// is the API's code (for example `JobType.Visibility` is 1). `str()` is the
/// name that the schema gives the code (for example "visibility").
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_enum
)]
#[pyclass(
    eq,
    eq_int,
    frozen,
    hash,
    from_py_object,
    name = "JobType",
    module = "mwa_giant_squid"
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PyJobType {
    Conversion = 0,
    Visibility = 1,
    Metadata = 2,
    Voltage = 3,
    Cancel = 4,
    Beamformer = 5,
    Imaging = 6,
}

/// Every member of [`PyJobType`], to find the member of a code.
const PY_JOB_TYPES: [PyJobType; 7] = [
    PyJobType::Conversion,
    PyJobType::Visibility,
    PyJobType::Metadata,
    PyJobType::Voltage,
    PyJobType::Cancel,
    PyJobType::Beamformer,
    PyJobType::Imaging,
];

impl From<PyJobType> for JobType {
    fn from(v: PyJobType) -> Self {
        JobType::try_from(v as i64).expect("each JobType member is a code of the schema")
    }
}

impl From<JobType> for PyJobType {
    fn from(v: JobType) -> Self {
        *PY_JOB_TYPES
            .iter()
            .find(|member| **member as i64 == *v)
            .expect("each code of the schema is a JobType member")
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyJobType {
    fn __str__(&self) -> String {
        JobType::from(*self).name().to_string()
    }
}

/// One file of a job's product: the OpenAPI schema's `JobFile`.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "JobFile",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyJobFile(JobFile);

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyJobFile {
    /// Where the file is delivered.
    #[getter]
    #[pyo3(name = "type")]
    fn type_(&self) -> PyType {
        self.0.type_.into()
    }

    /// The download URL (Acacia delivery), or `None`.
    #[getter]
    fn url(&self) -> Option<String> {
        self.0.url.clone()
    }

    /// The path on the filesystem (Scratch or DUG delivery), or `None`.
    #[getter]
    fn path(&self) -> Option<String> {
        self.0.path.clone()
    }

    /// The size of the file in bytes.
    #[getter]
    fn size(&self) -> i64 {
        self.0.size
    }

    /// The file's SHA-1 hash, or `None`.
    #[getter]
    fn sha1(&self) -> Option<String> {
        self.0.sha1.clone()
    }

    /// The file's format, as the MWA ASVO gives it, or `None`.
    #[getter]
    fn format(&self) -> Option<String> {
        self.0.format.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "JobFile(type={}, size={}, url={:?}, path={:?})",
            self.0.type_, self.0.size, self.0.url, self.0.path
        )
    }
}

/// The product of a completed job, its files: the OpenAPI schema's
/// `JobProduct`.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "JobProduct",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyJobProduct(JobProduct);

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyJobProduct {
    /// The job's files.
    #[getter]
    fn files(&self) -> Vec<PyJobFile> {
        self.0.files.iter().cloned().map(PyJobFile).collect()
    }

    fn __repr__(&self) -> String {
        format!("JobProduct(<{} files>)", self.0.files.len())
    }
}

/// An MWA ASVO job.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "AsvoJob",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyAsvoJob(AsvoJob);

impl From<AsvoJob> for PyAsvoJob {
    fn from(job: AsvoJob) -> Self {
        Self(job)
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoJob {
    /// The job ID.
    #[getter]
    fn job_id(&self) -> JobId {
        JobId(self.0.job_id())
    }

    /// The obsid.
    #[getter]
    fn obs_id(&self) -> u64 {
        u64::from(self.0.obs_id())
    }

    /// The job type, or `None` if the server gives none.
    #[getter]
    fn job_type(&self) -> Option<PyJobType> {
        self.0.job_type.map(PyJobType::from)
    }

    /// The job state.
    #[getter]
    fn job_state(&self) -> PyJobState {
        PyJobState::from(self.0.job_state)
    }

    /// The server's error code, or `None`. The MWA ASVO does not document
    /// its values, so it is given as it is.
    #[getter]
    fn error_code(&self) -> Option<i64> {
        self.0.error_code
    }

    /// The server's error message, or `None`. A job in the `Error` state
    /// has its message here.
    #[getter]
    fn error_text(&self) -> Option<String> {
        self.0.error_text.clone()
    }

    /// The job's product (its files), or `None` if the job has none yet.
    #[getter]
    fn product(&self) -> Option<PyJobProduct> {
        self.0.product.clone().map(PyJobProduct)
    }

    /// When the job completed (UTC), or `None`.
    #[getter]
    fn completed(&self) -> Option<Timestamp> {
        self.0.completed
    }

    /// When the job was created (UTC).
    #[getter]
    fn created(&self) -> Timestamp {
        self.0.created
    }

    /// When the job started (UTC), or `None` if it has not started.
    #[getter]
    fn started(&self) -> Option<Timestamp> {
        self.0.started
    }

    /// When the job was last changed (UTC), or `None`.
    #[getter]
    fn modified(&self) -> Option<Timestamp> {
        self.0.modified
    }

    /// The ID of the user who submitted the job.
    #[getter]
    fn user_id(&self) -> i64 {
        self.0.user_id
    }

    /// The first name of the user who submitted the job.
    #[getter]
    fn first_name(&self) -> String {
        self.0.first_name.clone()
    }

    /// The last name of the user who submitted the job.
    #[getter]
    fn last_name(&self) -> String {
        self.0.last_name.clone()
    }

    /// The job's parameters as the server gives them, as a `dict`.
    #[getter]
    fn job_params<'py>(&self, py: Python<'py>) -> PyResult<JsonDict<'py>> {
        let json = serde_json::to_string(&self.0.job_params)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        py.import("json")?
            .call_method1("loads", (json,))?
            .cast_into::<PyDict>()
            .map(JsonDict)
            .map_err(PyErr::from)
    }

    fn __repr__(&self) -> String {
        format!(
            "AsvoJob(job_id={}, obs_id={}, job_type={}, job_state={})",
            self.0.job_id(),
            self.0.obs_id(),
            self.0.job_type.map(|t| t.name()).unwrap_or("None"),
            self.0.job_state
        )
    }
}

/// A download progress event, given to the `progress` callback of
/// `AsvoClient.download_job` and `AsvoClient.download_obs`.
///
/// For each file there are one or more `Started` events, then zero or more
/// `Advanced` events, then one `Finished` event. A second `Started` for the
/// same file means that the download started again (for example, the
/// server did not honour a resume request), so reset the count. Each
/// variant is a subclass, so `isinstance` and `match` work.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "DownloadProgress",
    module = "mwa_giant_squid"
)]
#[derive(Clone, Debug, PartialEq)]
pub enum PyDownloadProgress {
    /// A file download starts, or starts again.
    Started {
        /// The MWA ASVO job ID.
        job_id: u64,
        /// A human-readable label, for example `Job ID 123 (obsid:
        /// 1234567890) [1/2]:`.
        label: String,
        /// The size of the file in bytes.
        total_bytes: u64,
        /// The bytes already on disk (not zero for a resumed download).
        position: u64,
    },
    /// `bytes` more bytes were written. Events are combined, so there are
    /// about 10 a second at most.
    Advanced {
        /// The number of bytes written since the last `Advanced` event.
        bytes: u64,
    },
    /// The file download is complete, or was skipped because the file is
    /// already on disk.
    Finished {},
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyDownloadProgress {
    fn __repr__(&self) -> String {
        match self {
            Self::Started {
                job_id,
                label,
                total_bytes,
                position,
            } => format!(
                "DownloadProgress.Started(job_id={job_id}, label={label:?}, total_bytes={total_bytes}, position={position})"
            ),
            Self::Advanced { bytes } => format!("DownloadProgress.Advanced(bytes={bytes})"),
            Self::Finished {} => "DownloadProgress.Finished()".to_string(),
        }
    }
}

impl From<DownloadProgress> for PyDownloadProgress {
    fn from(event: DownloadProgress) -> Self {
        match event {
            DownloadProgress::Started {
                job_id,
                label,
                total_bytes,
                position,
            } => Self::Started {
                job_id: job_id.get(),
                label,
                total_bytes,
                position,
            },
            DownloadProgress::Advanced { bytes } => Self::Advanced { bytes },
            DownloadProgress::Finished => Self::Finished {},
        }
    }
}

/// Define the Python class of a reply of the MWA ASVO that has the fields
/// `job_id`, `message` and `status`: `JobSubmittedResponse` and
/// `JobCancelledResponse` of the schema.
macro_rules! py_job_response {
    ($(#[$doc:meta])* $py:ident, $name:literal, $lib:ident) => {
        $(#[$doc])*
        #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
        #[pyclass(frozen, skip_from_py_object, name = $name, module = "mwa_giant_squid")]
        #[derive(Clone)]
        pub struct $py($lib);

        impl From<$lib> for $py {
            fn from(response: $lib) -> Self {
                Self(response)
            }
        }

        #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
        #[pymethods]
        impl $py {
            /// The ID of the job.
            #[getter]
            fn job_id(&self) -> JobId {
                JobId(self.0.job_id)
            }

            /// The server's message.
            #[getter]
            fn message(&self) -> String {
                self.0.message.clone()
            }

            /// The server's status for the request. For display only (see
            /// `Status`).
            #[getter]
            fn status(&self) -> PyStatus {
                self.0.status.into()
            }

            fn __repr__(&self) -> String {
                format!(
                    concat!($name, "(job_id={}, status={}, message={:?})"),
                    self.0.job_id, self.0.status, self.0.message
                )
            }
        }
    };
}

py_job_response!(
    /// The MWA ASVO's reply to a job submission: the OpenAPI schema's
    /// `JobSubmittedResponse`.
    PyJobSubmittedResponse,
    "JobSubmittedResponse",
    JobSubmittedResponse
);

py_job_response!(
    /// The MWA ASVO's reply to a cancellation: the OpenAPI schema's
    /// `JobCancelledResponse`. For a job that is already cancelled, the
    /// reply is normal and `status` is `Status.Failed`; read the message.
    PyJobCancelledResponse,
    "JobCancelledResponse",
    JobCancelledResponse
);

/// A list of MWA ASVO jobs. Supports `len()`, indexing and iteration.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, name = "AsvoJobVec", module = "mwa_giant_squid")]
pub struct PyAsvoJobVec(AsvoJobVec);

impl From<AsvoJobVec> for PyAsvoJobVec {
    fn from(jobs: AsvoJobVec) -> Self {
        Self(jobs)
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoJobVec {
    fn __len__(&self) -> usize {
        self.0 .0.len()
    }

    fn __getitem__(&self, index: isize) -> PyResult<PyAsvoJob> {
        let len = self.0 .0.len() as isize;
        let i = if index < 0 { index + len } else { index };
        if (0..len).contains(&i) {
            Ok(PyAsvoJob(self.0 .0[i as usize].clone()))
        } else {
            Err(PyIndexError::new_err("AsvoJobVec index out of range"))
        }
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<JobIterator<'py>> {
        let jobs: Vec<PyAsvoJob> = self.0 .0.iter().cloned().map(PyAsvoJob).collect();
        PyList::new(py, jobs)?.try_iter().map(JobIterator)
    }

    /// Keep only the jobs that match every given filter. A filter that is
    /// `None` or empty does not filter.
    ///
    /// Raises:
    ///     ValueError: An obsid is not valid.
    #[pyo3(signature = (job_ids=None, obs_ids=None, job_types=None, job_states=None))]
    fn filter(
        &self,
        job_ids: Option<Vec<JobId>>,
        obs_ids: Option<Vec<u64>>,
        job_types: Option<Vec<PyJobType>>,
        job_states: Option<Vec<PyJobState>>,
    ) -> PyResult<Self> {
        let obs_ids = obs_ids
            .unwrap_or_default()
            .into_iter()
            .map(|o| ObsId::validate(o).map_err(|e| PyValueError::new_err(e.to_string())))
            .collect::<PyResult<Vec<ObsId>>>()?;
        let job_types: Vec<JobType> = job_types
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        let job_states: Vec<JobState> = job_states
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(Self(
            self.0.clone().filter(
                &job_ids
                    .unwrap_or_default()
                    .into_iter()
                    .map(|id| id.0)
                    .collect::<Vec<_>>(),
                &obs_ids,
                &job_types,
                &job_states,
            ),
        ))
    }

    /// Whether all of `job_ids` are ready for download. `False` means some
    /// are still in progress.
    ///
    /// This makes no request: to wait, call `AsvoClient.get_jobs` and this
    /// in a loop.
    ///
    /// Raises:
    ///     AsvoError: A job is missing, has an error or has been cancelled.
    fn all_ready(&self, py: Python<'_>, job_ids: Vec<JobId>) -> PyResult<bool> {
        let job_ids: Vec<AsvoJobId> = job_ids.into_iter().map(|id| id.0).collect();
        self.0.all_ready(&job_ids).map_err(|e| asvo_error(py, e))
    }

    fn __repr__(&self) -> String {
        format!("AsvoJobVec(<{} jobs>)", self.0 .0.len())
    }
}
