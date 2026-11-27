// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The submission of one job for each of several Obs IDs from Python:
//! `MwaAsvoClient.submit_many`, its `params` argument and its result,
//! `SubmitResult`.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use super::csv::result_object;
use super::job_params::{
    PreprocessParams, PyConversionJobParams, PyDownloadJobParams, PyImagingJobParams,
    PyPreprocessJobParams,
};
use super::obs_id::PyObsId;
use super::typed::ExceptionObject;
use super::types::PyJobSubmittedResponse;
use crate::mwa_asvo::api::job_params::PreprocessJobParams;
use crate::mwa_asvo::api::openapi::DownloadJobParams;
use crate::SubmitManyResult;

/// The `params` argument of `submit_many`: a download body or a
/// preprocessing body, turned into the library's request body.
pub enum SubmitManyParams {
    /// A download body.
    Download(DownloadJobParams),
    /// A conversion body or an imaging body from an Obs ID.
    Preprocess(PreprocessJobParams),
}

impl<'a, 'py> FromPyObject<'a, 'py> for SubmitManyParams {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(params) = obj.cast::<PyDownloadJobParams>() {
            return Ok(Self::Download(params.borrow().to_schema()));
        }
        let is_preprocess = obj.is_instance_of::<PyPreprocessJobParams>()
            || obj.is_instance_of::<PyConversionJobParams>()
            || obj.is_instance_of::<PyImagingJobParams>();
        if is_preprocess {
            return Ok(Self::Preprocess(obj.extract::<PreprocessParams>()?.0));
        }
        Err(PyTypeError::new_err(format!(
            "expected a DownloadJobParams, a PreprocessJobParams, a ConversionJobParams or an \
             ImagingJobParams, not {}",
            obj.get_type().name()?
        )))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for SubmitManyParams {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo {
            name:
                "DownloadJobParams | PreprocessJobParams | ConversionJobParams | ImagingJobParams"
                    .to_string(),
            source_module: None,
            import: Default::default(),
            type_refs: std::collections::HashMap::new(),
        }
    }
}

/// The result of one Obs ID of `MwaAsvoClient.submit_many`: the Obs ID, and
/// the reply of the MWA ASVO or the exception of the submission.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, name = "SubmitResult", module = "mwa_giant_squid")]
pub struct PySubmitResult {
    obs_id: PyObsId,
    response: Option<Py<PyJobSubmittedResponse>>,
    error: Option<Py<PyAny>>,
}

impl PySubmitResult {
    /// The Python result of one result of the library.
    pub(super) fn from_lib(py: Python<'_>, (obs_id, result): SubmitManyResult) -> PyResult<Self> {
        let object = result_object(py, &result)?;
        let (response, error) = match result {
            Ok(_) => (
                Some(object.cast_into::<PyJobSubmittedResponse>()?.unbind()),
                None,
            ),
            Err(_) => (None, Some(object.unbind())),
        };
        Ok(Self {
            obs_id: PyObsId(obs_id),
            response,
            error,
        })
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PySubmitResult {
    /// The Obs ID of the job.
    #[getter]
    fn obs_id(&self) -> PyObsId {
        self.obs_id
    }

    /// The reply of the MWA ASVO, with the new job's ID, or `None` if the
    /// submission failed.
    #[getter]
    fn response(&self, py: Python<'_>) -> Option<Py<PyJobSubmittedResponse>> {
        self.response.as_ref().map(|r| r.clone_ref(py))
    }

    /// The exception of the submission (an `MwaAsvoApiError`), or `None` if
    /// it worked.
    #[getter]
    fn error(&self, py: Python<'_>) -> Option<ExceptionObject> {
        self.error
            .as_ref()
            .map(|e| ExceptionObject(e.clone_ref(py)))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let outcome = match (&self.response, &self.error) {
            (Some(response), _) => format!("response={}", response.bind(py).repr()?),
            (None, Some(error)) => format!("error={:?}", error.bind(py).str()?.to_string()),
            (None, None) => String::new(),
        };
        Ok(format!("SubmitResult(obs_id={}, {outcome})", self.obs_id.0))
    }
}
