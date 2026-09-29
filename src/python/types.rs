// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Python classes for the library's job types.
//!
//! Each class wraps the library type and has the same name and field names.
//! `AsvoJobState::Error(String)` carries data, so in Python it is the plain
//! enum member `AsvoJobState.Error` and the message is `AsvoJob.error_text`.

use chrono::{DateTime, Utc};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList};

use super::error::asvo_error;
use crate::asvo::{
    AsvoFilesArray, AsvoJob, AsvoJobID, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery,
};
use crate::obsid::Obsid;

/// Define a Python enum with the same members as a fieldless library enum,
/// and conversions in both directions.
macro_rules! py_enum {
    ($(#[$doc:meta])* $py:ident, $name:literal, $lib:ident, [$($variant:ident),+ $(,)?]) => {
        $(#[$doc])*
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

        #[pymethods]
        impl $py {
            fn __str__(&self) -> String {
                $lib::from(*self).to_string()
            }
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
    ]
);

py_enum!(
    /// Where the MWA ASVO delivers a job's files.
    PyDelivery,
    "Delivery",
    Delivery,
    [Acacia, Dug, Scratch]
);

/// The state of an MWA ASVO job. For `Error`, the message is in
/// `AsvoJob.error_text`.
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

#[pymethods]
impl PyAsvoJobState {
    fn __str__(&self) -> String {
        AsvoJobState::from(*self).to_string()
    }
}

/// One file of a job's product.
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "AsvoFilesArray",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyAsvoFilesArray(AsvoFilesArray);

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

    fn __repr__(&self) -> String {
        format!(
            "AsvoFilesArray(type={}, size={}, url={:?}, path={:?})",
            self.0.r#type, self.0.size, self.0.url, self.0.path
        )
    }
}

/// An MWA ASVO job.
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "AsvoJob",
    module = "mwa_giant_squid"
)]
#[derive(Clone)]
pub struct PyAsvoJob(AsvoJob);

#[pymethods]
impl PyAsvoJob {
    /// The job ID.
    #[getter]
    fn jobid(&self) -> AsvoJobID {
        self.0.jobid
    }

    /// The obsid.
    #[getter]
    fn obsid(&self) -> u64 {
        u64::from(self.0.obsid)
    }

    /// The job type.
    #[getter]
    fn jtype(&self) -> PyAsvoJobType {
        self.0.jtype.into()
    }

    /// The job state.
    #[getter]
    fn state(&self) -> PyAsvoJobState {
        PyAsvoJobState::from(&self.0.state)
    }

    /// The error message if the state is `Error`, otherwise `None`.
    #[getter]
    fn error_text(&self) -> Option<String> {
        match &self.0.state {
            AsvoJobState::Error(e) => Some(e.clone()),
            _ => None,
        }
    }

    /// The job's files, or `None` if the job has no product yet.
    #[getter]
    fn files(&self) -> Option<Vec<PyAsvoFilesArray>> {
        self.0
            .files
            .as_ref()
            .map(|files| files.iter().cloned().map(PyAsvoFilesArray).collect())
    }

    /// When the job completed (UTC), or `None`.
    #[getter]
    fn completed(&self) -> Option<DateTime<Utc>> {
        self.0.completed
    }

    fn __repr__(&self) -> String {
        format!(
            "AsvoJob(jobid={}, obsid={}, jtype={}, state={})",
            self.0.jobid, self.0.obsid, self.0.jtype, self.0.state
        )
    }
}

/// A list of MWA ASVO jobs. Supports `len()`, indexing and iteration.
#[pyclass(frozen, name = "AsvoJobVec", module = "mwa_giant_squid")]
pub struct PyAsvoJobVec(AsvoJobVec);

impl From<AsvoJobVec> for PyAsvoJobVec {
    fn from(jobs: AsvoJobVec) -> Self {
        Self(jobs)
    }
}

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

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let jobs: Vec<PyAsvoJob> = self.0 .0.iter().cloned().map(PyAsvoJob).collect();
        PyList::new(py, jobs)?.try_iter()
    }

    /// Keep only the jobs that match every given filter. A filter that is
    /// `None` or empty does not filter. States compare by kind only, so
    /// `AsvoJobState.Error` matches every job with an error.
    #[pyo3(signature = (jobids=None, obsids=None, jtypes=None, states=None))]
    fn filter(
        &self,
        jobids: Option<Vec<AsvoJobID>>,
        obsids: Option<Vec<u64>>,
        jtypes: Option<Vec<PyAsvoJobType>>,
        states: Option<Vec<PyAsvoJobState>>,
    ) -> PyResult<Self> {
        let obsids = obsids
            .unwrap_or_default()
            .into_iter()
            .map(|o| Obsid::validate(o).map_err(|e| PyValueError::new_err(e.to_string())))
            .collect::<PyResult<Vec<Obsid>>>()?;
        let jtypes: Vec<AsvoJobType> = jtypes
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        let states: Vec<AsvoJobState> = states
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(Self(self.0.clone().filter(
            &jobids.unwrap_or_default(),
            &obsids,
            &jtypes,
            &states,
        )))
    }

    /// Whether all of `jobids` are ready for download. `False` means some
    /// are still in progress. Raises `AsvoError` if one is missing, has an
    /// error, has expired or has been cancelled. This makes no request: to
    /// wait, call `AsvoClient.get_jobs` and this in a loop.
    fn all_ready(&self, py: Python<'_>, jobids: Vec<AsvoJobID>) -> PyResult<bool> {
        self.0.all_ready(&jobids).map_err(|e| asvo_error(py, e))
    }

    /// The jobs as a JSON object keyed by job ID, as `giant-squid list
    /// --json` prints.
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
