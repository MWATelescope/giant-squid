// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `AsvoClient`.
//!
//! Every network call releases the GIL (`Python::detach`), so other Python
//! threads run while it waits. The library client is `Send + Sync`, so one
//! Python `AsvoClient` can be used from several threads.

use std::path::PathBuf;

use pyo3::prelude::*;

use super::config::PyAsvoClientConfig;
use super::connect_python_logging;
use super::csv::csv_submit_error;
use super::download::{run_download, PyDownloadOptions};
use super::error::{api_error, asvo_error};
use super::filter::PyJobsFilter;
use super::job_params::{PreprocessParams, PyDownloadJobParams};
use super::obs_id::ObsIdArg;
use super::typed::JobId;
use super::types::{
    PyAsvoJob, PyAsvoJobVec, PyDeliveryDestination, PyDeliveryFormat, PyJobCancelledResponse,
    PyJobSubmittedResponse,
};
use crate::mwa_asvo::{AsvoClient, JobsFilter};
use crate::CsvDefaults;

/// A client for the MWA ASVO. It logs in when it is created.
///
/// Args:
///     config: The host, your API key, the API timeout and the token cache
///         (an `AsvoClientConfig`). `client_config_from_env` reads them from
///         the environment, as the `giant-squid` command does.
///
/// Raises:
///     AsvoApiError: The API key is empty (kind `MissingAuthKey`), or the
///         login failed.
///     ValueError: `config.api_timeout` is not finite, or is not more than 0.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, name = "AsvoClient", module = "mwa_giant_squid")]
pub struct PyAsvoClient {
    inner: AsvoClient,
    host: String,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoClient {
    #[new]
    fn new(py: Python<'_>, config: PyRef<'_, PyAsvoClientConfig>) -> PyResult<Self> {
        connect_python_logging();
        let host = config.host.clone();
        let lib_config = config.to_lib()?;
        let inner = py
            .detach(|| AsvoClient::new(lib_config))
            .map_err(|e| asvo_error(py, e))?;
        Ok(Self { inner, host })
    }

    /// Get your jobs, as `giant-squid list` does: the server filters what it
    /// can, and the lists (several Job IDs, Obs IDs, types or states) are
    /// applied to the result.
    ///
    /// Args:
    ///     filter: Which jobs to get (a `JobsFilter`). `None` is a filter
    ///         that does not filter, but note that `days=None` is the
    ///         default window of the MWA ASVO API, not your full history.
    ///
    /// Raises:
    ///     ValueError: The filter is not valid: both `job_ids` and
    ///         `obs_ids` are given, or `days` is not from 1 to 30 or is
    ///         given with `date_from` or `date_to`. (See `JobsFilter.validate`.)
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (filter=None))]
    fn get_jobs(
        &self,
        py: Python<'_>,
        filter: Option<PyRef<'_, PyJobsFilter>>,
    ) -> PyResult<PyAsvoJobVec> {
        let filter = filter.map_or_else(JobsFilter::default, |f| f.0.clone());
        py.detach(|| self.inner.get_jobs(&filter))
            .map(PyAsvoJobVec::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a download job: a visibility, metadata, voltage or
    /// beamformer job.
    ///
    /// The request body does not say which kind of download it is. The MWA
    /// ASVO finds that from the observation (and `only_get_metadata`), and
    /// it refuses a body that does not suit the observation: for example a
    /// delivery that the kind of observation does not allow, or `offset`,
    /// `duration` and the channels for an observation that is not a
    /// voltage observation. A voltage job is delivered to Scratch as
    /// files, which is not the default: give `delivery` and
    /// `delivery_format` in the `DownloadJobParams`.
    ///
    /// Args:
    ///     params: The request body.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ObsIdError: `params.obs_id` is not a valid Obs ID.
    ///     ValueError: `params.offset` is not from 0 to 5400, or `params.delivery` is
    ///         `DeliveryDestination.Acacia` and `params.delivery_format` is
    ///         `DeliveryFormat.Files` (Acacia delivers a tar only). Nothing
    ///         is sent.
    ///     AsvoApiError: The request failed.
    fn submit_download_job(
        &self,
        py: Python<'_>,
        params: PyRef<'_, PyDownloadJobParams>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = params.to_schema(py)?;
        py.detach(|| self.inner.submit_download_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a preprocessing job: a conversion job, an imaging job from an
    /// Obs ID (flow 1), or an imaging job from a conversion job (flow 2).
    ///
    /// The MWA ASVO finds the kind of job from the fields of the request
    /// body: only an imaging body has the imaging fields.
    ///
    /// Args:
    ///     params: The request body: a `ConversionJobParams`, an
    ///         `ImagingJobFlow1Params` or an `ImagingJobFlow2Params`.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     TypeError: `params` is not one of the three classes.
    ///     ObsIdError: An Obs ID is not valid.
    ///     ValueError: A Job ID is not valid, a number is outside
    ///         what the MWA ASVO accepts (for example `avg_freq_res` above
    ///         1280), `delivery` is `DeliveryDestination.Acacia` and
    ///         `delivery_format` is `DeliveryFormat.Files` (Acacia delivers a
    ///         tar only), or `centre` is `PhaseCentre.Custom` without both
    ///         `custom_centre_ra` and `custom_centre_dec`, or either of them
    ///         is given with another `centre`. The message names the
    ///         argument and its limits. Nothing is sent.
    ///     AsvoApiError: The request failed.
    fn submit_preprocess_job(
        &self,
        py: Python<'_>,
        params: PreprocessParams,
    ) -> PyResult<PyJobSubmittedResponse> {
        py.detach(|| self.inner.submit_preprocess_job(&params.0))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit the jobs of a manta-ray-client CSV file (the file that
    /// `mwa_client -c` reads), one job for each line.
    ///
    /// The whole file is read and checked before anything is submitted.
    /// Then the jobs are submitted in the order of the file. A job that
    /// fails does not stop the others.
    ///
    /// A key that a line does not give has the default of the MWA ASVO,
    /// except `delivery`, `delivery_format` and `allow_resubmit`, for which
    /// the arguments below are the defaults. (The module reads no
    /// environment variable, so `GIANT_SQUID_DELIVERY` is not used.) A key
    /// that a line does give wins. A key that a line gives twice, and a line
    /// that makes the same job as an earlier line, are logged as warnings.
    /// See the README for the keys of each job type.
    ///
    /// Args:
    ///     path: The CSV file.
    ///     delivery: The `delivery` of the lines that have none. `None`
    ///         leaves the default of the MWA ASVO.
    ///     delivery_format: The `delivery_format` of the lines that have
    ///         none. `None` leaves the default of the MWA ASVO.
    ///     allow_resubmit: The `allow_resubmit` of the lines that have none.
    ///         `None` leaves the default of the MWA ASVO.
    ///
    /// Returns:
    ///     The server's reply for each job, in the order of the file.
    ///
    /// Raises:
    ///     OSError: The file cannot be read. Nothing is submitted.
    ///     ValueError: The file has no job, or a line of the file has a
    ///         problem (for example an unknown key, or a value outside the
    ///         limits of the MWA ASVO). The message lists every bad line by
    ///         its line number. Nothing is submitted.
    ///     CsvSubmitError: Some jobs failed. The other jobs were submitted:
    ///         the exception's `submitted` has their replies, and its
    ///         `failures` has a `CsvJobFailure` for each job that failed.
    #[pyo3(
        text_signature = "($self, path, *, delivery=None, delivery_format=None, allow_resubmit=None)"
    )]
    #[pyo3(signature = (path, *, delivery=None, delivery_format=None, allow_resubmit=None))]
    fn submit_csv_file(
        &self,
        py: Python<'_>,
        path: PathBuf,
        delivery: Option<PyDeliveryDestination>,
        delivery_format: Option<PyDeliveryFormat>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<Vec<PyJobSubmittedResponse>> {
        let defaults = CsvDefaults {
            delivery: delivery.map(Into::into),
            delivery_format: delivery_format.map(Into::into),
            allow_resubmit,
        };
        py.detach(|| self.inner.submit_csv_file(&path, &defaults))
            .map(|replies| {
                replies
                    .into_iter()
                    .map(PyJobSubmittedResponse::from)
                    .collect()
            })
            .map_err(|e| csv_submit_error(py, e))
    }

    /// Cancel a job.
    ///
    /// Args:
    ///     job_id: The ID of the job to cancel.
    ///
    /// Returns:
    ///     The server's reply, a `JobCancelledResponse`. Its `job_id` is the
    ///     job in the request. The
    ///     MWA ASVO answers the cancellation of a job that is already
    ///     cancelled with a normal reply and not an error, so no exception
    ///     is raised. The reason is in `message`. Any other refusal is an
    ///     error.
    ///
    /// Raises:
    ///     ValueError: `job_id` is 0.
    ///     OverflowError: `job_id` is negative or too large to be a Job ID.
    ///     AsvoApiError: The request failed, for example because there is
    ///         no such job.
    fn cancel_job(&self, py: Python<'_>, job_id: JobId) -> PyResult<PyJobCancelledResponse> {
        py.detach(|| self.inner.cancel_job(job_id.0))
            .map(PyJobCancelledResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Download the files of a job.
    ///
    /// The job must be ready (see `AsvoJobVec.all_ready`). The call blocks
    /// until the download ends, with the GIL released. Ctrl-C stops the
    /// download at the next chunk and raises `KeyboardInterrupt`, when the
    /// call is made from the main thread. A partial file stays on disk, so
    /// a new call resumes it (unless `options.no_resume`).
    ///
    /// The call gets the job list first. To download several jobs, get the
    /// job list once and call `download_job_from` for each.
    ///
    /// Args:
    ///     job_id: The Job ID.
    ///     options: How to download, and where to (a `DownloadOptions`).
    ///
    /// Returns:
    ///     The job that was downloaded.
    ///
    /// Raises:
    ///     AsvoError: The job is missing, not ready or has no files, a
    ///         transfer failed, or the hash does not match.
    ///     AsvoApiError: Getting the job list failed.
    ///     ValueError: `options.download_dir`, `options.buffer_size` or
    ///         `options.retry_duration` is not valid.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_job(
        &self,
        py: Python<'_>,
        job_id: JobId,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyAsvoJob> {
        run_download(py, &options, |opts| self.inner.download_job(job_id.0, opts))
            .map(PyAsvoJob::from)
    }

    /// Download the files of a job that is in a job list that you have.
    ///
    /// As `download_job`, but it finds the job in `jobs`, and makes no
    /// request to get the job list.
    ///
    /// Args:
    ///     jobs: A job list from `get_jobs`.
    ///     job_id: The Job ID.
    ///     options: How to download, and where to (a `DownloadOptions`).
    ///
    /// Returns:
    ///     The job that was downloaded.
    ///
    /// Raises:
    ///     AsvoError: The job is missing from `jobs`, not ready or has no
    ///         files, a transfer failed, or the hash does not match.
    ///     ValueError: `options.download_dir`, `options.buffer_size` or
    ///         `options.retry_duration` is not valid.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_job_from(
        &self,
        py: Python<'_>,
        jobs: PyRef<'_, PyAsvoJobVec>,
        job_id: JobId,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyAsvoJob> {
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.inner().clone();
        run_download(py, &options, |opts| {
            self.inner.download_job_from(&jobs, job_id.0, opts)
        })
        .map(PyAsvoJob::from)
    }

    /// Download the files of the one ready job for an Obs ID.
    ///
    /// The way the download runs is that of `download_job`.
    ///
    /// Args:
    ///     obs_id: The Obs ID. There must be exactly one ready job for it.
    ///     options: How to download, and where to (a `DownloadOptions`).
    ///
    /// Returns:
    ///     The job that was downloaded (its `job_id` is the job found for
    ///     the Obs ID).
    ///
    /// Raises:
    ///     ObsIdError: `obs_id` is not a valid Obs ID.
    ///     AsvoError: No job, no ready job, or more than one ready job has
    ///         this Obs ID, or the download failed (see `download_job`).
    ///     AsvoApiError: Getting the job list failed.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_obs(
        &self,
        py: Python<'_>,
        obs_id: ObsIdArg,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyAsvoJob> {
        run_download(py, &options, |opts| self.inner.download_obs(obs_id.0, opts))
            .map(PyAsvoJob::from)
    }

    /// Download the files of the one ready job for an Obs ID, from a job
    /// list that you have.
    ///
    /// As `download_obs`, but it finds the job in `jobs`, and makes no
    /// request to get the job list.
    ///
    /// Args:
    ///     jobs: A job list from `get_jobs`.
    ///     obs_id: The Obs ID. There must be exactly one ready job for it.
    ///     options: How to download, and where to (a `DownloadOptions`).
    ///
    /// Returns:
    ///     The job that was downloaded.
    ///
    /// Raises:
    ///     ObsIdError: `obs_id` is not a valid Obs ID.
    ///     AsvoError: No job, no ready job, or more than one ready job has
    ///         this Obs ID, or the download failed (see `download_job`).
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_obs_from(
        &self,
        py: Python<'_>,
        jobs: PyRef<'_, PyAsvoJobVec>,
        obs_id: ObsIdArg,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyAsvoJob> {
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.inner().clone();
        run_download(py, &options, |opts| {
            self.inner.download_obs_from(&jobs, obs_id.0, opts)
        })
        .map(PyAsvoJob::from)
    }

    fn __repr__(&self) -> String {
        let host = &self.host;
        format!("AsvoClient(host={host:?})")
    }
}
