// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `MwaAsvoClient`.
//!
//! Every network call releases the GIL (`Python::detach`), so other Python
//! threads run while it waits. The library client is `Send + Sync`, so one
//! Python `MwaAsvoClient` can be used from several threads.

use std::path::PathBuf;

use pyo3::prelude::*;

use super::config::PyMwaAsvoClientConfig;
use super::connect_python_logging;
use super::csv::{
    csv_jobs_failed, csv_submit_error, result_object, PyCsvDefaults, PyCsvJob, PyCsvJobParams,
};
use super::download::{run_download, PyDownloadOptions};
use super::error::{api_error, mwa_asvo_error};
use super::filter::PyJobsFilter;
use super::job_params::{PreprocessParams, PyDownloadJobParams, PyImageFromJobParams};
use super::obs_id::ObsIdArg;
use super::typed::{JobId, ResultCallback};
use super::types::{PyJobCancelledResponse, PyJobSubmittedResponse, PyMwaAsvoJob, PyMwaAsvoJobVec};
use crate::mwa_asvo::{JobsFilter, MwaAsvoClient};

/// A client for the MWA ASVO. It logs in when it is created.
///
/// Args:
///     config: The host, your API key, the API timeout and the token cache
///         (an `MwaAsvoClientConfig`). `client_config_from_env` reads them from
///         the environment, as the `giant-squid` command does.
///
/// Raises:
///     MwaAsvoErrorApi: The API key is empty, or the login failed. Its
///         `source` is the `MwaAsvoApiError` (kind `MissingAuthKey` or
///         `AuthenticationFailed`, for example).
///     MwaAsvoErrorInvalidOption: `config.api_timeout` is 0.
///     ValueError: `config.api_timeout` is negative or not finite.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(frozen, name = "MwaAsvoClient", module = "mwa_giant_squid")]
pub struct PyMwaAsvoClient {
    inner: MwaAsvoClient,
    host: String,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyMwaAsvoClient {
    #[new]
    fn new(py: Python<'_>, config: PyRef<'_, PyMwaAsvoClientConfig>) -> PyResult<Self> {
        connect_python_logging();
        let host = config.host.clone();
        let lib_config = config.to_lib()?;
        let inner = py
            .detach(|| MwaAsvoClient::new(lib_config))
            .map_err(|e| mwa_asvo_error(py, e))?;
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
    ///     MwaAsvoApiErrorInvalidParameter: The filter is not valid: both
    ///         `job_ids` and `obs_ids` are given, or `days` is not from 1 to
    ///         30 or is given with `date_from` or `date_to`. (See
    ///         `JobsFilter.validate`.)
    ///     MwaAsvoApiError: The request failed.
    #[pyo3(signature = (filter=None))]
    fn get_jobs(
        &self,
        py: Python<'_>,
        filter: Option<PyRef<'_, PyJobsFilter>>,
    ) -> PyResult<PyMwaAsvoJobVec> {
        let filter = filter.map_or_else(JobsFilter::default, |f| f.0.clone());
        py.detach(|| self.inner.get_jobs(&filter))
            .map(PyMwaAsvoJobVec::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a download job: a visibility, metadata, voltage or
    /// beamformer job.
    ///
    /// The MWA ASVO finds the kind of data from the observation. It
    /// refuses a body that does not suit the observation (for example
    /// `offset` and `duration` for an observation that has no voltages).
    ///
    /// Args:
    ///     params: The request body.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     MwaAsvoApiErrorInvalidParameter: The `offset` is not from 0 to
    ///         7200. Nothing is sent.
    ///     MwaAsvoApiError: The request failed.
    fn submit_download_job(
        &self,
        py: Python<'_>,
        params: PyRef<'_, PyDownloadJobParams>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = params.to_schema();
        py.detach(|| self.inner.submit_download_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a preprocessing job: a conversion job or an imaging job from
    /// an Obs ID. An imaging job from a conversion job is
    /// `submit_image_from_job`.
    ///
    /// The MWA ASVO finds the kind of job from the fields of the request
    /// body: only an imaging body has the imaging fields.
    ///
    /// Args:
    ///     params: The request body: a `PreprocessJobParams`, a
    ///         `ConversionJobParams` or an `ImagingJobParams`.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     TypeError: `params` is not one of the three classes.
    ///     MwaAsvoApiErrorInvalidParameter: A number is outside what the MWA
    ///         ASVO accepts (for example `avg_freq_res` above 1280). The
    ///         message names the argument and its limits. Nothing is sent.
    ///     MwaAsvoApiError: The request failed.
    fn submit_preprocess_job(
        &self,
        py: Python<'_>,
        params: PreprocessParams,
    ) -> PyResult<PyJobSubmittedResponse> {
        py.detach(|| self.inner.submit_preprocess_job(&params.0))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit an imaging job from a conversion job: an image of
    /// the conversion job `params.source_job_id`. The MWA ASVO takes the Obs
    /// ID from the source job.
    ///
    /// Args:
    ///     params: The request body.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     MwaAsvoApiErrorInvalidParameter: A number is outside what the MWA
    ///         ASVO accepts. The message names the argument and its limits.
    ///         Nothing is sent.
    ///     MwaAsvoApiError: The request failed.
    fn submit_image_from_job(
        &self,
        py: Python<'_>,
        params: PyRef<'_, PyImageFromJobParams>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = params.to_schema(py)?;
        py.detach(|| self.inner.submit_image_from_job(&params))
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
    /// `defaults` are the defaults. (This method reads no environment
    /// variable. `CsvDefaults.from_env()` reads the variables of the
    /// `giant-squid` command.) A key that a line does give wins. A key that
    /// a line gives twice, and a line that makes the same job as an earlier
    /// line, are logged as warnings. See the README for the keys of each job
    /// type.
    ///
    /// Args:
    ///     path: The CSV file.
    ///     defaults: What the lines do not give (a `CsvDefaults`). `None` is
    ///         no defaults.
    ///
    /// Returns:
    ///     The server's reply for each job, in the order of the file.
    ///
    /// Raises:
    ///     OSError: The file cannot be read. Nothing is submitted.
    ///     CsvSubmitError: The file has a problem on some lines (kind
    ///         `Invalid`, with `errors`, the message lists every bad line by
    ///         its line number), or it has no job (kind `NoJobs`). Nothing is
    ///         submitted. Or some jobs failed (kind `Failed`): the other jobs
    ///         were submitted, and the exception's `submitted` has their
    ///         replies and its `failures` has a `CsvJobFailure` for each job
    ///         that failed.
    #[pyo3(signature = (path, defaults=None))]
    fn submit_csv_file(
        &self,
        py: Python<'_>,
        path: PathBuf,
        defaults: Option<PyRef<'_, PyCsvDefaults>>,
    ) -> PyResult<Vec<PyJobSubmittedResponse>> {
        let defaults = PyCsvDefaults::lib_or_default(defaults);
        py.detach(|| self.inner.submit_csv_file(&path, &defaults))
            .map(|replies| {
                replies
                    .into_iter()
                    .map(PyJobSubmittedResponse::from)
                    .collect()
            })
            .map_err(|e| csv_submit_error(py, e))
    }

    /// Submit the jobs that `parse_manta_ray_csv` or `read_manta_ray_csv`
    /// read, in order, and carry on past a job that fails.
    ///
    /// Args:
    ///     jobs: The jobs (`CsvJob`).
    ///     on_result: A function to call for each job when its request is
    ///         done, so that you can report it at once. It is called with
    ///         the `CsvJob` and its result: the `JobSubmittedResponse`, or
    ///         the `MwaAsvoApiError` that the request failed with (as a value,
    ///         not raised). If it raises, it is not called again, the other
    ///         jobs are still submitted (the library's loop cannot be
    ///         stopped), and its first exception is raised when they are
    ///         done.
    ///
    /// Returns:
    ///     The server's reply for each job, in the order of the jobs.
    ///
    /// Raises:
    ///     MwaAsvoApiErrorInvalidParameter: The image size or `nmiter` of an
    ///         imaging job is not valid. Nothing is submitted.
    ///     CsvJobsFailed: Some jobs failed. The other jobs were submitted:
    ///         the exception's `submitted` has their replies and its
    ///         `failures` has a `CsvJobFailure` for each job that failed.
    #[pyo3(signature = (jobs, on_result=None))]
    fn submit_csv_jobs(
        &self,
        py: Python<'_>,
        jobs: Vec<Py<PyCsvJob>>,
        on_result: Option<ResultCallback>,
    ) -> PyResult<Vec<PyJobSubmittedResponse>> {
        let lib_jobs = jobs
            .iter()
            .map(|job| job.get().to_lib(py))
            .collect::<PyResult<Vec<_>>>()?;
        let mut done = 0;
        let mut callback_error: Option<PyErr> = None;
        let result = py.detach(|| {
            self.inner.submit_csv_jobs(&lib_jobs, |_, result| {
                let index = done;
                done += 1;
                let Some(callback) = &on_result else { return };
                if callback_error.is_some() {
                    return;
                }
                Python::attach(|py| {
                    let outcome = result_object(py, result).and_then(|result| {
                        callback
                            .0
                            .bind(py)
                            .call1((jobs[index].clone_ref(py), result))
                    });
                    if let Err(e) = outcome {
                        callback_error = Some(e);
                    }
                });
            })
        });
        if let Some(e) = callback_error {
            return Err(e);
        }
        result
            .map(|replies| {
                replies
                    .into_iter()
                    .map(PyJobSubmittedResponse::from)
                    .collect()
            })
            .map_err(|e| csv_jobs_failed(py, e))
    }

    /// Submit the job of one line of a manta-ray-client CSV file (see
    /// `parse_manta_ray_csv`), to the endpoint of its job type.
    ///
    /// Args:
    ///     params: The request body (a `CsvJobParams`, from a `CsvJob`).
    ///
    /// Returns:
    ///     The server's reply.
    ///
    /// Raises:
    ///     MwaAsvoApiErrorInvalidParameter: A value is outside the limits of
    ///         the MWA ASVO. Nothing is sent.
    ///     MwaAsvoApiError: The request failed. (The errors of the submit
    ///         methods.)
    fn submit_csv_job(
        &self,
        py: Python<'_>,
        params: Py<PyCsvJobParams>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = params.get().to_lib(py)?;
        py.detach(|| self.inner.submit_csv_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
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
    ///     MwaAsvoApiError: The request failed, for example because there is
    ///         no such job.
    fn cancel_job(&self, py: Python<'_>, job_id: JobId) -> PyResult<PyJobCancelledResponse> {
        py.detach(|| self.inner.cancel_job(job_id.0))
            .map(PyJobCancelledResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Download the files of a job.
    ///
    /// The job must be ready (see `MwaAsvoJobVec.all_ready`). The call blocks
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
    ///     MwaAsvoError: The job is missing, not ready or has no files, a
    ///         transfer failed, or the hash does not match.
    ///     MwaAsvoErrorApi: Getting the job list failed. Its `source` is the
    ///         `MwaAsvoApiError`.
    ///     MwaAsvoErrorInvalidOption: `options.buffer_size` is not valid.
    ///     ValueError: `options.download_dir` is not valid UTF-8, or
    ///         `options.retry_duration` is negative or not finite.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_job(
        &self,
        py: Python<'_>,
        job_id: JobId,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyMwaAsvoJob> {
        run_download(py, &options, |opts| self.inner.download_job(job_id.0, opts))
            .map(PyMwaAsvoJob::from)
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
    ///     MwaAsvoError: The job is missing from `jobs`, not ready or has no
    ///         files, a transfer failed, or the hash does not match.
    ///     MwaAsvoErrorInvalidOption: `options.buffer_size` is not valid.
    ///     ValueError: `options.download_dir` is not valid UTF-8, or
    ///         `options.retry_duration` is negative or not finite.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_job_from(
        &self,
        py: Python<'_>,
        jobs: PyRef<'_, PyMwaAsvoJobVec>,
        job_id: JobId,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyMwaAsvoJob> {
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.inner().clone();
        run_download(py, &options, |opts| {
            self.inner.download_job_from(&jobs, job_id.0, opts)
        })
        .map(PyMwaAsvoJob::from)
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
    ///     MwaAsvoError: No job, no ready job, or more than one ready job has
    ///         this Obs ID, or the download failed (see `download_job`).
    ///     MwaAsvoErrorApi: Getting the job list failed. Its `source` is the
    ///         `MwaAsvoApiError`.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_obs(
        &self,
        py: Python<'_>,
        obs_id: ObsIdArg,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyMwaAsvoJob> {
        run_download(py, &options, |opts| self.inner.download_obs(obs_id.0, opts))
            .map(PyMwaAsvoJob::from)
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
    ///     MwaAsvoError: No job, no ready job, or more than one ready job has
    ///         this Obs ID, or the download failed (see `download_job`).
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    fn download_obs_from(
        &self,
        py: Python<'_>,
        jobs: PyRef<'_, PyMwaAsvoJobVec>,
        obs_id: ObsIdArg,
        options: PyRef<'_, PyDownloadOptions>,
    ) -> PyResult<PyMwaAsvoJob> {
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.inner().clone();
        run_download(py, &options, |opts| {
            self.inner.download_obs_from(&jobs, obs_id.0, opts)
        })
        .map(PyMwaAsvoJob::from)
    }

    fn __repr__(&self) -> String {
        let host = &self.host;
        format!("MwaAsvoClient(host={host:?})")
    }
}
