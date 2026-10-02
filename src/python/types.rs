// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Python classes for the library's job types.
//!
//! Each class wraps the library type and has the same name and field names.
//! `AsvoJobState::Error(String)` carries data, so in Python it is the plain
//! enum member `AsvoJobState.Error` and the message is `AsvoJob.error_text`.

use jiff::Timestamp;
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::error::asvo_error;
use super::typed::{JobIterator, JsonDict};
use crate::asvo::apiv2::openapi::{
    self as api, Centre, DeliveryFormat, JobSubmittedResponse, Output, OutputMode, Polarization,
    Weighting,
};
use crate::asvo::{
    AsvoFilesArray, AsvoJob, AsvoJobId, AsvoJobProduct, AsvoJobState, AsvoJobType, AsvoJobVec,
    Delivery, DownloadProgress, DownloadSettings,
};
use crate::obs_id::ObsId;

/// Define a Python enum with the same members as a fieldless library enum,
/// and conversions in both directions. An optional last argument, in braces,
/// adds methods to the Python class (a class can have only one block of
/// methods).
macro_rules! py_enum {
    ($(#[$doc:meta])* $py:ident, $name:literal, $lib:ident, [$($variant:ident),+ $(,)?] $(, { $($extra:tt)* })?) => {
        $(#[$doc])*
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

            $($($extra)*)?
        }
    };
}

py_enum!(
    /// The type of an MWA ASVO job.
    PyAsvoJobType,
    "AsvoJobType",
    AsvoJobType,
    [
        Conversion,
        DownloadVisibilities,
        DownloadMetadata,
        DownloadVoltage,
        CancelJob,
        DownloadBeamformer,
        Imaging,
        Unknown,
    ],
    {
        /// The names of the job types that `parse` accepts (and that the
        /// `--job-types` option of `giant-squid list` offers), in the order
        /// the help lists them. `Unknown` has no name.
        #[staticmethod]
        fn names() -> Vec<String> {
            AsvoJobType::names().into_iter().map(String::from).collect()
        }

        /// Get a job type from its name. The case, spaces, hyphens and
        /// underscores do not matter, so `download_visibilities` and
        /// `DownloadVisibilities` are the same type. `download_voltage` and
        /// `download_voltages` both give `DownloadVoltage`.
        ///
        /// Raises:
        ///     AsvoError: the text is not the name of a job type, `unknown`
        ///         included (kind `InvalidJobType`).
        #[staticmethod]
        fn parse(py: Python<'_>, text: &str) -> PyResult<Self> {
            text.parse::<AsvoJobType>()
                .map(Self::from)
                .map_err(|e| asvo_error(py, e))
        }
    }
);

py_enum!(
    /// Where the MWA ASVO delivers a job's files.
    PyDelivery,
    "Delivery",
    Delivery,
    [Acacia, Dug, Scratch]
);

impl From<PyDelivery> for api::Delivery {
    /// `Delivery` is also the type of the `delivery` argument of the job
    /// submit methods, which the library types as the OpenAPI enum.
    fn from(d: PyDelivery) -> Self {
        match d {
            PyDelivery::Acacia => Self::Acacia,
            PyDelivery::Dug => Self::Dug,
            PyDelivery::Scratch => Self::Scratch,
        }
    }
}

// The enums below are the job arguments of the submit methods. Their
// members are those of the OpenAPI schema, and `str()` of a member is the
// value the API uses (for example "uvfits").

py_enum!(
    /// How the MWA ASVO packages a job's files: one tar file, or separate
    /// files. `str()` is the API value.
    PyDeliveryFormat,
    "DeliveryFormat",
    DeliveryFormat,
    [Tar, Files]
);

py_enum!(
    /// The format of a conversion job's output. `str()` is the API value.
    PyOutput,
    "Output",
    Output,
    [Ms, Uvfits]
);

py_enum!(
    /// Where to put the phase centre of a conversion job or an imaging job.
    /// `str()` is the API value.
    PyCentre,
    "Centre",
    Centre,
    [Phase, Pointing, Custom]
);

py_enum!(
    /// The products an imaging job returns. `str()` is the API value.
    PyOutputMode,
    "OutputMode",
    OutputMode,
    [Fits, AllFits, AllFiles]
);

py_enum!(
    /// The WSClean weighting scheme of an imaging job. `str()` is the API
    /// value.
    PyWeighting,
    "Weighting",
    Weighting,
    [Briggs, Uniform, Natural]
);

py_enum!(
    /// The polarisation an imaging job images. `str()` is the API value.
    PyPolarization,
    "Polarization",
    Polarization,
    [Xx, Yy, Xxyy]
);

/// The state of an MWA ASVO job. For `Error`, the message is in
/// `AsvoJob.error_text`.
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
    name = "AsvoJobState",
    module = "mwa_giant_squid"
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PyAsvoJobState {
    Queued,
    WaitCal,
    Staging,
    Staged,
    Preparing,
    Downloading,
    Preprocessing,
    Imaging,
    Delivering,
    Ready,
    Error,
    Expired,
    Cancelled,
}

impl From<&AsvoJobState> for PyAsvoJobState {
    fn from(s: &AsvoJobState) -> Self {
        match s {
            AsvoJobState::Queued => Self::Queued,
            AsvoJobState::WaitCal => Self::WaitCal,
            AsvoJobState::Staging => Self::Staging,
            AsvoJobState::Staged => Self::Staged,
            AsvoJobState::Preparing => Self::Preparing,
            AsvoJobState::Downloading => Self::Downloading,
            AsvoJobState::Preprocessing => Self::Preprocessing,
            AsvoJobState::Imaging => Self::Imaging,
            AsvoJobState::Delivering => Self::Delivering,
            AsvoJobState::Ready => Self::Ready,
            AsvoJobState::Error(_) => Self::Error,
            AsvoJobState::Expired => Self::Expired,
            AsvoJobState::Cancelled => Self::Cancelled,
        }
    }
}

impl From<PyAsvoJobState> for AsvoJobState {
    /// `Error` has no message here. That is correct for filtering, which
    /// compares only the kind of state.
    fn from(s: PyAsvoJobState) -> Self {
        match s {
            PyAsvoJobState::Queued => Self::Queued,
            PyAsvoJobState::WaitCal => Self::WaitCal,
            PyAsvoJobState::Staging => Self::Staging,
            PyAsvoJobState::Staged => Self::Staged,
            PyAsvoJobState::Preparing => Self::Preparing,
            PyAsvoJobState::Downloading => Self::Downloading,
            PyAsvoJobState::Preprocessing => Self::Preprocessing,
            PyAsvoJobState::Imaging => Self::Imaging,
            PyAsvoJobState::Delivering => Self::Delivering,
            PyAsvoJobState::Ready => Self::Ready,
            PyAsvoJobState::Error => Self::Error(String::new()),
            PyAsvoJobState::Expired => Self::Expired,
            PyAsvoJobState::Cancelled => Self::Cancelled,
        }
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoJobState {
    fn __str__(&self) -> String {
        AsvoJobState::from(*self).to_string()
    }

    /// The names of the job states that `parse` accepts (and that the
    /// `--job-states` option of `giant-squid list` offers), in the order the
    /// help lists them.
    #[staticmethod]
    fn names() -> Vec<String> {
        AsvoJobState::names()
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// Get a job state from its name. The case, spaces, hyphens and
    /// underscores do not matter, so `WAIT-CAL` and `waitcal` are the same
    /// state.
    ///
    /// Raises:
    ///     AsvoError: the text is not the name of a job state (kind
    ///         `InvalidJobState`).
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Self> {
        text.parse::<AsvoJobState>()
            .map(|state| PyAsvoJobState::from(&state))
            .map_err(|e| asvo_error(py, e))
    }
}

/// One file of a job's product.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "AsvoFilesArray",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyAsvoFilesArray(AsvoFilesArray);

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoFilesArray {
    /// Where the file is delivered.
    #[getter]
    #[pyo3(name = "type")]
    fn delivery_type(&self) -> PyDelivery {
        self.0.r#type.into()
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
    fn size(&self) -> u64 {
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
            "AsvoFilesArray(type={}, size={}, url={:?}, path={:?})",
            self.0.r#type, self.0.size, self.0.url, self.0.path
        )
    }
}

/// The product of a completed job: its files.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "AsvoJobProduct",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyAsvoJobProduct(AsvoJobProduct);

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoJobProduct {
    /// The job's files.
    #[getter]
    fn files(&self) -> Vec<PyAsvoFilesArray> {
        self.0.files.iter().cloned().map(PyAsvoFilesArray).collect()
    }

    fn __repr__(&self) -> String {
        format!("AsvoJobProduct(<{} files>)", self.0.files.len())
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

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoJob {
    /// The job ID.
    #[getter]
    fn job_id(&self) -> AsvoJobId {
        self.0.job_id
    }

    /// The obsid.
    #[getter]
    fn obs_id(&self) -> u64 {
        u64::from(self.0.obs_id)
    }

    /// The job type.
    #[getter]
    fn job_type(&self) -> PyAsvoJobType {
        self.0.job_type.into()
    }

    /// The job state.
    #[getter]
    fn job_state(&self) -> PyAsvoJobState {
        PyAsvoJobState::from(&self.0.job_state)
    }

    /// The job state as text for a person: the state, and for a job in the
    /// `Error` state `Error: <message>`.
    #[getter]
    fn state_text(&self) -> String {
        self.0.job_state.to_string()
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
    fn product(&self) -> Option<PyAsvoJobProduct> {
        self.0.product.clone().map(PyAsvoJobProduct)
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
            self.0.job_id, self.0.obs_id, self.0.job_type, self.0.job_state
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
        job_id: AsvoJobId,
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
                job_id,
                label,
                total_bytes,
                position,
            },
            DownloadProgress::Advanced { bytes } => Self::Advanced { bytes },
            DownloadProgress::Finished => Self::Finished {},
        }
    }
}

/// The download settings that the `giant-squid` command reads from the
/// environment, to pass to `AsvoClient.download_job` and `download_obs`.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "DownloadSettings",
    module = "mwa_giant_squid"
)]
pub struct PyDownloadSettings(DownloadSettings);

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyDownloadSettings {
    /// Read the settings from the environment.
    ///
    /// `GIANT_SQUID_BUF_SIZE` is a whole number of MiB. `GIANT_SQUID_DOWNLOAD_RETRY_SECS`
    /// is a whole number of seconds; a value that is not one is logged as a
    /// warning and the default is used. A setting that is not set has the
    /// library's default.
    ///
    /// Raises:
    ///     AsvoError: `GIANT_SQUID_BUF_SIZE` is set but is not a whole
    ///         number of MiB, or is too large (kind `InvalidEnvironment`).
    #[staticmethod]
    fn from_env(py: Python<'_>) -> PyResult<Self> {
        DownloadSettings::from_env()
            .map(Self)
            .map_err(|e| asvo_error(py, e))
    }

    /// How many bytes to hold in memory before they are written. Pass it as
    /// `buffer_size`.
    #[getter]
    fn buffer_size(&self) -> usize {
        self.0.buffer_size
    }

    /// How long to retry a failing download, in seconds. Pass it as
    /// `retry_duration`.
    #[getter]
    fn retry_duration(&self) -> f64 {
        self.0.retry_duration.as_secs_f64()
    }

    fn __repr__(&self) -> String {
        format!(
            "DownloadSettings(buffer_size={}, retry_duration={})",
            self.0.buffer_size,
            self.0.retry_duration.as_secs_f64()
        )
    }
}

/// The MWA ASVO's reply to a job submission or to a cancellation.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "JobSubmittedResponse",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyJobSubmittedResponse(JobSubmittedResponse);

impl From<JobSubmittedResponse> for PyJobSubmittedResponse {
    fn from(response: JobSubmittedResponse) -> Self {
        Self(response)
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyJobSubmittedResponse {
    /// The ID of the job that was submitted (or cancelled).
    #[getter]
    fn job_id(&self) -> u64 {
        self.0.job_id.get()
    }

    /// The server's message.
    #[getter]
    fn message(&self) -> String {
        self.0.message.clone()
    }

    /// The server's status text for the request, "success" or "failed". It
    /// describes the reply, like `message`; it is for display only. Success
    /// or failure of a call is decided by the HTTP status, so a call that
    /// fails raises `AsvoApiError`, and this text is not to be used to
    /// decide whether a call worked.
    #[getter]
    fn status(&self) -> String {
        self.0.status.to_string()
    }

    /// The response as one line of JSON with the keys `job_id`, `message`
    /// and `status`, as `giant-squid submit-vis --json` prints it.
    fn json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "JobSubmittedResponse(job_id={}, status={:?}, message={:?})",
            self.0.job_id,
            self.0.status.to_string(),
            self.0.message
        )
    }
}

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
    /// `None` or empty does not filter. States compare by kind only, so
    /// `AsvoJobState.Error` matches every job with an error.
    ///
    /// Raises:
    ///     ValueError: An obsid is not valid.
    #[pyo3(signature = (job_ids=None, obs_ids=None, job_types=None, job_states=None))]
    fn filter(
        &self,
        job_ids: Option<Vec<AsvoJobId>>,
        obs_ids: Option<Vec<u64>>,
        job_types: Option<Vec<PyAsvoJobType>>,
        job_states: Option<Vec<PyAsvoJobState>>,
    ) -> PyResult<Self> {
        let obs_ids = obs_ids
            .unwrap_or_default()
            .into_iter()
            .map(|o| ObsId::validate(o).map_err(|e| PyValueError::new_err(e.to_string())))
            .collect::<PyResult<Vec<ObsId>>>()?;
        let job_types: Vec<AsvoJobType> = job_types
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        let job_states: Vec<AsvoJobState> = job_states
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(Self(self.0.clone().filter(
            &job_ids.unwrap_or_default(),
            &obs_ids,
            &job_types,
            &job_states,
        )))
    }

    /// Whether all of `job_ids` are ready for download. `False` means some
    /// are still in progress.
    ///
    /// This makes no request: to wait, call `AsvoClient.get_jobs` and this
    /// in a loop.
    ///
    /// Raises:
    ///     AsvoError: A job is missing, has an error, has expired or has
    ///         been cancelled.
    fn all_ready(&self, py: Python<'_>, job_ids: Vec<AsvoJobId>) -> PyResult<bool> {
        self.0.all_ready(&job_ids).map_err(|e| asvo_error(py, e))
    }

    /// The jobs as a JSON object keyed by job ID, as `giant-squid list
    /// --json` prints. The keys are the OpenAPI names (`obs_id`, `job_id`,
    /// `job_type`, `job_state`, `product`, `created`, `started`,
    /// `completed`, `modified`, `error_code`, `error_text`, `user_id`, `first_name`,
    /// `last_name`, `job_params`; and, in `product.files`, `type`, `url`,
    /// `path`, `size`, `sha1`, `format` for each file).
    fn json(&self) -> PyResult<String> {
        self.0
            .clone()
            .json()
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!("AsvoJobVec(<{} jobs>)", self.0 .0.len())
    }
}
