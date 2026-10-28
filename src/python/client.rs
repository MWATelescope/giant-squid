// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `AsvoClient`.
//!
//! Every network call releases the GIL (`Python::detach`), so other Python
//! threads run while it waits. The library client is `Send + Sync`, so one
//! Python `AsvoClient` can be used from several threads.

use std::path::PathBuf;
use std::time::Duration;

use jiff::Timestamp;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::connect_python_logging;
use super::csv::csv_submit_error;
use super::download::{run_download, PyDownloadArgs};
use super::error::{api_error, asvo_error};
use super::job_params::{PreprocessParams, PyDownloadJobParams};
use super::obs_id::ObsIdArg;
use super::typed::{JobId, ProgressCallback};
use super::types::{
    PyAsvoJob, PyAsvoJobVec, PyDeliveryDestination, PyDeliveryFormat, PyJobCancelledResponse,
    PyJobState, PyJobSubmittedResponse, PyJobType, PySortBy,
};
use crate::mwa_asvo::api::validate;
use crate::mwa_asvo::{AsvoClient, AsvoClientConfig, JobsFilter};
use crate::obs_id::ObsId;
use crate::CsvDefaults;

/// A client for the MWA ASVO. It logs in when it is created.
///
/// Args:
///     host: The MWA ASVO base URL, for example
///         "https://asvo.mwatelescope.org:443". An "http://" host is
///         permitted (a local test server); every other host must use TLS.
///     api_key: Your MWA ASVO API key.
///     api_timeout: The timeout for one API request, in seconds. More
///         than 0. `None` uses the library default (`DEFAULT_API_TIMEOUT` of the Rust
///         library).
///     token_cache_path: Where to cache the session between runs. `None`
///         keeps the session in memory only, so every new client logs in.
///         Give a path for a script that runs often, because the server
///         permits only a few logins a minute. A cached session is used
///         only by a client with the same `host` and `api_key`; the file
///         holds a hash of the key, never the key.
///
/// Raises:
///     AsvoApiError: The API key is empty, or the login failed.
///     ValueError: `api_timeout` is not finite, or is not more than 0.
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
    #[pyo3(signature = (host, api_key, *, api_timeout=None, token_cache_path=None))]
    fn new(
        py: Python<'_>,
        host: String,
        api_key: String,
        api_timeout: Option<f64>,
        token_cache_path: Option<PathBuf>,
    ) -> PyResult<Self> {
        connect_python_logging();
        let mut config = AsvoClientConfig::new(host.clone(), api_key);
        if let Some(seconds) = api_timeout {
            config.api_timeout = Duration::try_from_secs_f64(seconds).map_err(|e| {
                PyValueError::new_err(format!("api_timeout={seconds} is not valid: {e}"))
            })?;
        }
        config.token_cache_path = token_cache_path;

        let inner = py
            .detach(|| AsvoClient::new(config))
            .map_err(|e| asvo_error(py, e))?;
        Ok(Self { inner, host })
    }

    /// Get your jobs, as `giant-squid list` does: the server filters what it
    /// can, and the lists (several Job IDs, Obs IDs, types or states) are
    /// applied to the result. Every filter that is `None` does not filter,
    /// except `days` and `sort_by`: `days=None` is the default window of the
    /// MWA ASVO API, not your full history.
    ///
    /// Args:
    ///     job_ids: Only these jobs. Cannot be combined with `obs_ids`.
    ///     obs_ids: Only the jobs for these Obs IDs.
    ///     job_types: Only the jobs of these types.
    ///     job_states: Only the jobs in these states.
    ///     days: Only the jobs from the past `days` days, from 1 to 30.
    ///         Cannot be given with `date_from` or `date_to`: the jobs are
    ///         selected by `days` or by the dates. `None` is the default of
    ///         the MWA ASVO API (30 days), unless a date is given, and then
    ///         `days` is sent as `null`.
    ///     date_from: Only the jobs created at or after this time. It must
    ///         have a time zone.
    ///     date_to: Only the jobs created at or before this time. It must
    ///         have a time zone.
    ///     sort_by: The column to sort the jobs by (a `SortBy`).
    ///         `None` uses the server's default order.
    ///
    /// Raises:
    ///     ObsIdError: An Obs ID is not valid.
    ///     ValueError: Both `job_ids` and `obs_ids` are given, or `days` is
    ///         not from 1 to 30 or is given with `date_from` or `date_to`.
    ///     TypeError: `date_from` or `date_to` has no time zone.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (
        job_ids=None,
        obs_ids=None,
        job_types=None,
        job_states=None,
        *,
        days=None,
        date_from=None,
        date_to=None,
        sort_by=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn get_jobs(
        &self,
        py: Python<'_>,
        job_ids: Option<Vec<JobId>>,
        obs_ids: Option<Vec<ObsIdArg>>,
        job_types: Option<Vec<PyJobType>>,
        job_states: Option<Vec<PyJobState>>,
        days: Option<i64>,
        date_from: Option<Timestamp>,
        date_to: Option<Timestamp>,
        sort_by: Option<PySortBy>,
    ) -> PyResult<PyAsvoJobVec> {
        let obs_ids = obs_ids
            .unwrap_or_default()
            .into_iter()
            .map(|o| o.0)
            .collect::<Vec<ObsId>>();
        let days = days
            .map(validate::days)
            .transpose()
            .map_err(|e| api_error(py, e))?;
        let filter = JobsFilter {
            days,
            date_from,
            date_to,
            sort_by: sort_by.map(Into::into),
            job_ids: job_ids
                .unwrap_or_default()
                .into_iter()
                .map(|id| id.0)
                .collect(),
            obs_ids,
            job_types: job_types
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            job_states: job_states
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        };
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
    /// a new call resumes it (unless `no_resume`).
    ///
    /// Args:
    ///     job_id: The Job ID.
    ///     download_dir: The directory for the files. It must exist.
    ///     keep_tar: Keep the tar file as it is. `False` unpacks it into
    ///         `download_dir` while it downloads.
    ///     job_subdirectory: Put the files in a directory of their own,
    ///         `download_dir/<job_id>`, which is made if it is not there,
    ///         and not in `download_dir` itself, as the `giant-squid`
    ///         command does. The files of two jobs of the same observation
    ///         do not meet in one directory. `False` puts the files in
    ///         `download_dir` itself, as giant-squid 2.x did. It has no
    ///         effect on a Scratch job, which is moved into `download_dir`
    ///         in a folder of its own.
    ///     no_resume: Download the whole file again, even if part of it is
    ///         on disk.
    ///     hash: Check the SHA-1 hash of the file against the MWA ASVO's. A
    ///         resumed download, and a complete file that is already on
    ///         disk, are always checked, even when this is `False`.
    ///     progress: A function to call with each `DownloadProgress` event,
    ///         or `None`. `Advanced` events are combined, so the function is
    ///         called about 10 times a second at most. If it raises, the
    ///         download stops and its exception is raised.
    ///     buffer_size: How many bytes to hold in memory before they are
    ///         written. At least 1. `None` uses the library default
    ///         (`DEFAULT_DOWNLOAD_BUFFER_SIZE` of the Rust library).
    ///     retry_duration: How long to retry a failing download, in
    ///         seconds. 0 disables retries. `None` uses the library default
    ///         (`DEFAULT_DOWNLOAD_RETRY_DURATION` of the Rust library).
    ///     download_number: The number of this download, in a series,
    ///         for the progress and log label (`[1/2]`).
    ///     download_count: How many downloads there are in the series.
    ///     jobs: A job list from `get_jobs` to find the job
    ///         in. `None` gets the job list first. To download several jobs,
    ///         get the job list once and pass it to each call.
    ///
    /// Returns:
    ///     The job that was downloaded.
    ///
    /// Raises:
    ///     AsvoError: The job is missing, not ready or has no files, a
    ///         transfer failed, or the hash does not match.
    ///     AsvoApiError: Getting the job list failed (only when `jobs` is
    ///         `None`).
    ///     ValueError: `download_dir`, `buffer_size` or `retry_duration` is not valid.
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    #[pyo3(signature = (
        job_id,
        download_dir,
        *,
        keep_tar=false,
        job_subdirectory=true,
        no_resume=false,
        hash=true,
        progress=None,
        buffer_size=None,
        retry_duration=None,
        download_number=1,
        download_count=1,
        jobs=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn download_job(
        &self,
        py: Python<'_>,
        job_id: JobId,
        download_dir: PathBuf,
        keep_tar: bool,
        job_subdirectory: bool,
        no_resume: bool,
        hash: bool,
        progress: Option<ProgressCallback>,
        buffer_size: Option<usize>,
        retry_duration: Option<f64>,
        download_number: usize,
        download_count: usize,
        jobs: Option<PyRef<'_, PyAsvoJobVec>>,
    ) -> PyResult<PyAsvoJob> {
        let args = PyDownloadArgs {
            download_dir,
            job_subdirectory,
            keep_tar,
            no_resume,
            hash,
            progress: progress.map(|p| p.0),
            buffer_size,
            retry_duration,
            download_number,
            download_count,
        };
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.map(|jobs| jobs.inner().clone());
        run_download(py, args, |opts| match &jobs {
            Some(jobs) => self.inner.download_job_from(jobs, job_id.0, opts),
            None => self.inner.download_job(job_id.0, opts),
        })
        .map(PyAsvoJob::from)
    }

    /// Download the files of the one ready job for an Obs ID.
    ///
    /// The arguments, and the way the download runs, are those of
    /// `download_job`.
    ///
    /// Args:
    ///     obs_id: The Obs ID. There must be exactly one ready job for it.
    ///
    /// Returns:
    ///     The job that was downloaded (its `job_id` is the job found for
    ///     the Obs ID).
    ///
    /// Raises:
    ///     ObsIdError: `obs_id` is not a valid Obs ID.
    ///     AsvoError: No job, no ready job, or more than one ready job has
    ///         this Obs ID, or the download failed (see `download_job`).
    ///     AsvoApiError: Getting the job list failed (only when `jobs` is
    ///         `None`).
    ///     KeyboardInterrupt: Ctrl-C was pressed.
    #[pyo3(signature = (
        obs_id,
        download_dir,
        *,
        keep_tar=false,
        job_subdirectory=true,
        no_resume=false,
        hash=true,
        progress=None,
        buffer_size=None,
        retry_duration=None,
        download_number=1,
        download_count=1,
        jobs=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn download_obs(
        &self,
        py: Python<'_>,
        obs_id: ObsIdArg,
        download_dir: PathBuf,
        keep_tar: bool,
        job_subdirectory: bool,
        no_resume: bool,
        hash: bool,
        progress: Option<ProgressCallback>,
        buffer_size: Option<usize>,
        retry_duration: Option<f64>,
        download_number: usize,
        download_count: usize,
        jobs: Option<PyRef<'_, PyAsvoJobVec>>,
    ) -> PyResult<PyAsvoJob> {
        let obs_id = obs_id.0;
        let args = PyDownloadArgs {
            download_dir,
            job_subdirectory,
            keep_tar,
            no_resume,
            hash,
            progress: progress.map(|p| p.0),
            buffer_size,
            retry_duration,
            download_number,
            download_count,
        };
        // A copy, so that the download can run without the GIL.
        let jobs = jobs.map(|jobs| jobs.inner().clone());
        run_download(py, args, |opts| match &jobs {
            Some(jobs) => self.inner.download_obs_from(jobs, obs_id, opts),
            None => self.inner.download_obs(obs_id, opts),
        })
        .map(PyAsvoJob::from)
    }

    fn __repr__(&self) -> String {
        let host = &self.host;
        format!("AsvoClient(host={host:?})")
    }
}
