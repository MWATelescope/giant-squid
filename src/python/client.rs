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
use super::params::job_obs_id;
use super::typed::{JobId, ProgressCallback};
use super::types::{
    PyAsvoJob, PyAsvoJobVec, PyCentre, PyDelivery, PyDeliveryFormat, PyJobCancelledResponse,
    PyJobState, PyJobSubmittedResponse, PyJobType, PyOutput, PyOutputMode, PyPolarization,
    PyWeighting,
};
use crate::mwa_asvo::api::job_params as schema;
use crate::mwa_asvo::api::openapi::DownloadType;
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
    ///     sort_by: The column to sort the jobs by, for example "id".
    ///         `None` uses the server's default order.
    ///
    /// Raises:
    ///     ValueError: Both `job_ids` and `obs_ids` are given, an Obs ID is
    ///         not valid, or `days` is not from 1 to 30 or is given with
    ///         `date_from` or `date_to`.
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
        obs_ids: Option<Vec<u64>>,
        job_types: Option<Vec<PyJobType>>,
        job_states: Option<Vec<PyJobState>>,
        days: Option<i64>,
        date_from: Option<Timestamp>,
        date_to: Option<Timestamp>,
        sort_by: Option<String>,
    ) -> PyResult<PyAsvoJobVec> {
        let obs_ids = obs_ids
            .unwrap_or_default()
            .into_iter()
            .map(job_obs_id)
            .collect::<PyResult<Vec<ObsId>>>()?;
        let days = days
            .map(validate::days)
            .transpose()
            .map_err(|e| api_error(py, e))?;
        let filter = JobsFilter {
            days,
            date_from,
            date_to,
            sort_by,
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

    /// Submit a job to download an observation's raw visibilities.
    ///
    /// Every keyword argument has the MWA ASVO's default as its default
    /// value (the type stubs show it). `None` is accepted by the arguments
    /// that the MWA ASVO allows to be `null`, and is sent as `null`: it
    /// does not mean "use the default".
    ///
    /// Args:
    ///     obs_id: The Obs ID.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files. Must be
    ///         `DeliveryFormat.Tar` when `delivery` is `Delivery.Acacia`.
    ///     allow_resubmit: Submit the job even if an identical one has
    ///         completed.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, or `delivery` is
    ///         `Delivery.Acacia` and `delivery_format` is
    ///         `DeliveryFormat.Files` (Acacia delivers a tar only). Nothing
    ///         is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDelivery::from(schema::download_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::download_defaults().delivery_format),
        allow_resubmit = schema::download_defaults().allow_resubmit,
    ))]
    fn submit_download_vis_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let mut params = schema::download_params(job_obs_id(obs_id)?, DownloadType::Vis);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_download_vis_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a job to download an observation's metadata.
    ///
    /// The arguments are those of `submit_download_vis_job`.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, or `delivery` is
    ///         `Delivery.Acacia` and `delivery_format` is
    ///         `DeliveryFormat.Files` (Acacia delivers a tar only). Nothing
    ///         is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDelivery::from(schema::download_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::download_defaults().delivery_format),
        allow_resubmit = schema::download_defaults().allow_resubmit,
    ))]
    fn submit_download_meta_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let mut params = schema::download_params(job_obs_id(obs_id)?, DownloadType::Meta);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_download_meta_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a conversion (preprocessing) job.
    ///
    /// Every keyword argument has the MWA ASVO's default as its default
    /// value (the type stubs show it). `None` is accepted by the arguments
    /// that the MWA ASVO allows to be `null`, and is sent as `null`: it
    /// does not mean "use the default".
    ///
    /// Args:
    ///     obs_id: The Obs ID.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files. Must be
    ///         `DeliveryFormat.Tar` when `delivery` is `Delivery.Acacia`.
    ///     output: The output format.
    ///     avg_freq_res: The frequency resolution to average to (kHz).
    ///     avg_time_res: The time resolution to average to (s).
    ///     flag_edge_width: The width of the frequency edge flagging (kHz).
    ///     apply_di_cal: Apply the DI calibration solution.
    ///     centre: The phase centre mode. `Centre.Custom` needs
    ///         `custom_centre_ra` and `custom_centre_dec`, and they are only
    ///         allowed with it.
    ///     custom_centre_ra: The custom phase centre's right ascension
    ///         (degrees).
    ///     custom_centre_dec: The custom phase centre's declination
    ///         (degrees).
    ///     no_apply_amps: Do not apply the amplitude calibration solutions.
    ///     no_digital_gains: Do not apply the digital gains.
    ///     no_flag_dc: Do not flag the DC channel.
    ///     no_geometric_delay: Do not apply the geometric delay corrections.
    ///     no_passband_gains: Do not apply the passband gain corrections.
    ///     no_cable_delay: Do not apply the cable delay corrections.
    ///     no_rfi: Do not flag RFI.
    ///     allow_resubmit: Submit the job even if an identical one has
    ///         completed.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, a number is outside
    ///         what the MWA ASVO accepts (for example `avg_freq_res` above
    ///         1280), `delivery` is `Delivery.Acacia` and `delivery_format`
    ///         is `DeliveryFormat.Files` (Acacia delivers a tar only), or
    ///         `centre` is `Centre.Custom` without both `custom_centre_ra`
    ///         and `custom_centre_dec`, or either of them is given with
    ///         another `centre`. The message names the argument and
    ///         its limits. Nothing is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        output='Output.Ms', \
        avg_freq_res=40.0, \
        avg_time_res=2.0, \
        flag_edge_width=80.0, \
        apply_di_cal=False, \
        centre='Centre.Phase', \
        custom_centre_ra=None, \
        custom_centre_dec=None, \
        no_apply_amps=False, \
        no_digital_gains=False, \
        no_flag_dc=False, \
        no_geometric_delay=False, \
        no_passband_gains=False, \
        no_cable_delay=False, \
        no_rfi=False, \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDelivery::from(schema::conversion_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::conversion_defaults().delivery_format),
        output = PyOutput::from(schema::conversion_defaults().output),
        avg_freq_res = schema::conversion_defaults().avg_freq_res,
        avg_time_res = schema::conversion_defaults().avg_time_res,
        flag_edge_width = schema::conversion_defaults().flag_edge_width,
        apply_di_cal = schema::conversion_defaults().apply_di_cal,
        centre = PyCentre::from(schema::conversion_defaults().centre),
        custom_centre_ra = schema::conversion_defaults().custom_centre_ra,
        custom_centre_dec = schema::conversion_defaults().custom_centre_dec,
        no_apply_amps = schema::conversion_defaults().no_apply_amps,
        no_digital_gains = schema::conversion_defaults().no_digital_gains,
        no_flag_dc = schema::conversion_defaults().no_flag_dc,
        no_geometric_delay = schema::conversion_defaults().no_geometric_delay,
        no_passband_gains = schema::conversion_defaults().no_passband_gains,
        no_cable_delay = schema::conversion_defaults().no_cable_delay,
        no_rfi = schema::conversion_defaults().no_rfi,
        allow_resubmit = schema::conversion_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_conversion_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        output: PyOutput,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        flag_edge_width: f64,
        apply_di_cal: bool,
        centre: PyCentre,
        custom_centre_ra: Option<f64>,
        custom_centre_dec: Option<f64>,
        no_apply_amps: bool,
        no_digital_gains: Option<bool>,
        no_flag_dc: Option<bool>,
        no_geometric_delay: Option<bool>,
        no_passband_gains: Option<bool>,
        no_cable_delay: Option<bool>,
        no_rfi: Option<bool>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let mut params = schema::conversion_params(job_obs_id(obs_id)?);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.output = output.into();
        params.avg_freq_res = avg_freq_res;
        params.avg_time_res = avg_time_res;
        params.flag_edge_width = flag_edge_width;
        params.apply_di_cal = apply_di_cal;
        params.centre = centre.into();
        params.custom_centre_ra = custom_centre_ra;
        params.custom_centre_dec = custom_centre_dec;
        params.no_apply_amps = no_apply_amps;
        params.no_digital_gains = no_digital_gains;
        params.no_flag_dc = no_flag_dc;
        params.no_geometric_delay = no_geometric_delay;
        params.no_passband_gains = no_passband_gains;
        params.no_cable_delay = no_cable_delay;
        params.no_rfi = no_rfi;
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_conversion_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit an imaging job that starts from an Obs ID.
    ///
    /// Every keyword argument has the MWA ASVO's default as its default
    /// value (the type stubs show it). `None` is accepted by the arguments
    /// that the MWA ASVO allows to be `null`, and is sent as `null`: it
    /// does not mean "use the default".
    ///
    /// Args:
    ///     obs_id: The Obs ID.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files. Must be
    ///         `DeliveryFormat.Tar` when `delivery` is `Delivery.Acacia`.
    ///     apply_di_cal: Apply the DI calibration solution.
    ///     apply_primary_beam: Apply the primary beam correction.
    ///     auto_mask: The WSClean -auto-mask value.
    ///     auto_threshold: The WSClean -auto-threshold value.
    ///     abs_threshold: The absolute cleaning threshold (Jy).
    ///     avg_freq_res: The frequency resolution to average to before
    ///         imaging (kHz).
    ///     avg_time_res: The time resolution to average to before imaging
    ///         (s).
    ///     channels_out: The number of output channel groups.
    ///     clean_iterations: The WSClean -niter value.
    ///     clean_threshold: The WSClean cleaning threshold (Jy). This
    ///         field is deprecated in the API: use `abs_threshold`.
    ///     centre: Where to centre the image. `Centre.Custom` needs
    ///         `custom_centre_ra` and `custom_centre_dec`, and they are only
    ///         allowed with it.
    ///     custom_centre_dec: The custom phase centre's declination
    ///         (degrees).
    ///     custom_centre_ra: The custom phase centre's right ascension
    ///         (degrees).
    ///     flag_edge_width: The width of the frequency edge flagging (kHz).
    ///     image_size: The WSClean image size in pixels. Only the sizes
    ///         that the MWA ASVO accepts are valid.
    ///     join_channels: Join the output channel groups for cleaning.
    ///     join_polarizations: Join the polarisations for cleaning.
    ///     mgain: The WSClean -mgain value.
    ///     multiscale: Use WSClean multiscale cleaning.
    ///     nmiter: The WSClean -nmiter value. Must be greater than zero.
    ///     no_apply_amps: Do not apply the amplitude calibration solutions.
    ///     no_digital_gains: Do not apply the digital gains.
    ///     no_flag_dc: Do not flag the DC channel.
    ///     no_geometric_delay: Do not apply the geometric delay corrections.
    ///     no_passband_gains: Do not apply the passband gain corrections.
    ///     no_cable_delay: Do not apply the cable delay corrections.
    ///     no_rfi: Do not flag RFI.
    ///     nwlayers: The number of w-projection layers. This field is
    ///         deprecated in the API: use `wstack_nwlayers`.
    ///     output_mode: The products to return.
    ///     pixel_scale: The pixel scale (arcsec per pixel).
    ///     pol: The polarisation to image.
    ///     robust: The WSClean -robust value.
    ///     uvw_max: The maximum uv distance to image (wavelengths).
    ///     uvw_min: The minimum uv distance to image (wavelengths).
    ///     weighting: The WSClean weighting scheme.
    ///     wstack_nwlayers: The number of w-stacking layers.
    ///     allow_resubmit: Submit the job even if an identical one has
    ///         completed.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, or an argument is
    ///         outside what the MWA ASVO accepts (for example `mgain`
    ///         above 1, or an `image_size` that is not a supported size),
    ///         `delivery` is `Delivery.Acacia` and `delivery_format` is
    ///         `DeliveryFormat.Files` (Acacia delivers a tar only), or
    ///         `centre` is `Centre.Custom` without both `custom_centre_ra`
    ///         and `custom_centre_dec`, or either of them is given with
    ///         another `centre`. The message names the argument and
    ///         its limits. Nothing is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        apply_di_cal=True, \
        apply_primary_beam=True, \
        auto_mask=3, \
        auto_threshold=0.5, \
        abs_threshold=0.001, \
        avg_freq_res=40.0, \
        avg_time_res=2.0, \
        channels_out=4, \
        clean_iterations=100000, \
        clean_threshold=0.001, \
        centre='Centre.Phase', \
        custom_centre_dec=None, \
        custom_centre_ra=None, \
        flag_edge_width=80.0, \
        image_size=3072, \
        join_channels=True, \
        join_polarizations=False, \
        mgain=0.8, \
        multiscale=False, \
        nmiter=10, \
        no_apply_amps=False, \
        no_digital_gains=False, \
        no_flag_dc=False, \
        no_geometric_delay=False, \
        no_passband_gains=False, \
        no_cable_delay=False, \
        no_rfi=False, \
        nwlayers=None, \
        output_mode='OutputMode.Fits', \
        pixel_scale=20.0, \
        pol='Polarization.Xxyy', \
        robust=-0.5, \
        uvw_max=None, \
        uvw_min=75.0, \
        weighting='Weighting.Briggs', \
        wstack_nwlayers=None, \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDelivery::from(schema::imaging_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::imaging_defaults().delivery_format),
        apply_di_cal = schema::imaging_defaults().apply_di_cal,
        apply_primary_beam = schema::imaging_defaults().apply_primary_beam,
        auto_mask = schema::imaging_defaults().auto_mask,
        auto_threshold = schema::imaging_defaults().auto_threshold,
        abs_threshold = schema::imaging_defaults().abs_threshold,
        avg_freq_res = schema::imaging_defaults().avg_freq_res,
        avg_time_res = schema::imaging_defaults().avg_time_res,
        channels_out = schema::imaging_defaults().channels_out,
        clean_iterations = schema::imaging_defaults().clean_iterations,
        clean_threshold = schema::imaging_defaults().clean_threshold,
        centre = PyCentre::from(schema::imaging_defaults().centre),
        custom_centre_dec = schema::imaging_defaults().custom_centre_dec,
        custom_centre_ra = schema::imaging_defaults().custom_centre_ra,
        flag_edge_width = schema::imaging_defaults().flag_edge_width,
        image_size = i64::from(schema::imaging_defaults().image_size),
        join_channels = schema::imaging_defaults().join_channels,
        join_polarizations = schema::imaging_defaults().join_polarizations,
        mgain = schema::imaging_defaults().mgain,
        multiscale = schema::imaging_defaults().multiscale,
        nmiter = schema::imaging_defaults().nmiter.get(),
        no_apply_amps = schema::imaging_defaults().no_apply_amps,
        no_digital_gains = schema::imaging_defaults().no_digital_gains,
        no_flag_dc = schema::imaging_defaults().no_flag_dc,
        no_geometric_delay = schema::imaging_defaults().no_geometric_delay,
        no_passband_gains = schema::imaging_defaults().no_passband_gains,
        no_cable_delay = schema::imaging_defaults().no_cable_delay,
        no_rfi = schema::imaging_defaults().no_rfi,
        nwlayers = schema::imaging_defaults().nwlayers,
        output_mode = PyOutputMode::from(schema::imaging_defaults().output_mode),
        pixel_scale = schema::imaging_defaults().pixel_scale,
        pol = PyPolarization::from(schema::imaging_defaults().pol),
        robust = schema::imaging_defaults().robust,
        uvw_max = schema::imaging_defaults().uvw_max,
        uvw_min = schema::imaging_defaults().uvw_min,
        weighting = PyWeighting::from(schema::imaging_defaults().weighting),
        wstack_nwlayers = schema::imaging_defaults().wstack_nwlayers,
        allow_resubmit = schema::imaging_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_imaging_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        apply_di_cal: bool,
        apply_primary_beam: bool,
        auto_mask: i64,
        auto_threshold: f64,
        abs_threshold: Option<f64>,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        channels_out: i64,
        clean_iterations: i64,
        clean_threshold: Option<f64>,
        centre: PyCentre,
        custom_centre_dec: Option<f64>,
        custom_centre_ra: Option<f64>,
        flag_edge_width: f64,
        image_size: i64,
        join_channels: bool,
        join_polarizations: bool,
        mgain: f64,
        multiscale: bool,
        nmiter: u64,
        no_apply_amps: bool,
        no_digital_gains: Option<bool>,
        no_flag_dc: Option<bool>,
        no_geometric_delay: Option<bool>,
        no_passband_gains: Option<bool>,
        no_cable_delay: Option<bool>,
        no_rfi: Option<bool>,
        nwlayers: Option<i64>,
        output_mode: PyOutputMode,
        pixel_scale: f64,
        pol: PyPolarization,
        robust: f64,
        uvw_max: Option<f64>,
        uvw_min: f64,
        weighting: PyWeighting,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let image_size = validate::image_size(image_size).map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(nmiter).map_err(|e| api_error(py, e))?;
        let mut params = schema::imaging_params(job_obs_id(obs_id)?);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.apply_di_cal = apply_di_cal;
        params.apply_primary_beam = apply_primary_beam;
        params.auto_mask = auto_mask;
        params.auto_threshold = auto_threshold;
        params.abs_threshold = abs_threshold;
        params.avg_freq_res = avg_freq_res;
        params.avg_time_res = avg_time_res;
        params.channels_out = channels_out;
        params.clean_iterations = clean_iterations;
        params.clean_threshold = clean_threshold;
        params.centre = centre.into();
        params.custom_centre_dec = custom_centre_dec;
        params.custom_centre_ra = custom_centre_ra;
        params.flag_edge_width = flag_edge_width;
        params.image_size = image_size;
        params.join_channels = join_channels;
        params.join_polarizations = join_polarizations;
        params.mgain = mgain;
        params.multiscale = multiscale;
        params.nmiter = nmiter;
        params.no_apply_amps = no_apply_amps;
        params.no_digital_gains = no_digital_gains;
        params.no_flag_dc = no_flag_dc;
        params.no_geometric_delay = no_geometric_delay;
        params.no_passband_gains = no_passband_gains;
        params.no_cable_delay = no_cable_delay;
        params.no_rfi = no_rfi;
        params.nwlayers = nwlayers;
        params.output_mode = output_mode.into();
        params.pixel_scale = pixel_scale;
        params.pol = pol.into();
        params.robust = robust;
        params.uvw_max = uvw_max;
        params.uvw_min = uvw_min;
        params.weighting = weighting.into();
        params.wstack_nwlayers = wstack_nwlayers;
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_imaging_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit an imaging job that starts from an existing conversion job.
    ///
    /// Every keyword argument has the MWA ASVO's default as its default
    /// value (the type stubs show it). `None` is accepted by the arguments
    /// that the MWA ASVO allows to be `null`, and is sent as `null`: it
    /// does not mean "use the default".
    /// The arguments are those of `submit_imaging_job`, except that the
    /// data come from a conversion job, so there are no calibration,
    /// averaging, flagging or phase centre arguments.
    ///
    /// Args:
    ///     obs_id: The Obs ID.
    ///     source_job_id: The ID of the conversion job to image. Must be
    ///         greater than zero.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, `source_job_id` is
    ///         zero, or an argument is outside what the MWA ASVO accepts or
    ///         does not agree with another (see `submit_imaging_job`).
    ///         Nothing is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, source_job_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        apply_primary_beam=True, \
        auto_mask=3, \
        auto_threshold=0.5, \
        abs_threshold=0.001, \
        channels_out=4, \
        clean_iterations=100000, \
        clean_threshold=0.001, \
        image_size=3072, \
        join_channels=True, \
        join_polarizations=False, \
        mgain=0.8, \
        multiscale=False, \
        nmiter=10, \
        nwlayers=None, \
        output_mode='OutputMode.Fits', \
        pixel_scale=20.0, \
        pol='Polarization.Xxyy', \
        robust=-0.5, \
        uvw_max=None, \
        uvw_min=75.0, \
        weighting='Weighting.Briggs', \
        wstack_nwlayers=None, \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        source_job_id,
        *,
        delivery = PyDelivery::from(schema::image_from_job_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::image_from_job_defaults().delivery_format),
        apply_primary_beam = schema::image_from_job_defaults().apply_primary_beam,
        auto_mask = schema::image_from_job_defaults().auto_mask,
        auto_threshold = schema::image_from_job_defaults().auto_threshold,
        abs_threshold = schema::image_from_job_defaults().abs_threshold,
        channels_out = schema::image_from_job_defaults().channels_out,
        clean_iterations = schema::image_from_job_defaults().clean_iterations,
        clean_threshold = schema::image_from_job_defaults().clean_threshold,
        image_size = i64::from(schema::image_from_job_defaults().image_size),
        join_channels = schema::image_from_job_defaults().join_channels,
        join_polarizations = schema::image_from_job_defaults().join_polarizations,
        mgain = schema::image_from_job_defaults().mgain,
        multiscale = schema::image_from_job_defaults().multiscale,
        nmiter = schema::image_from_job_defaults().nmiter.get(),
        nwlayers = schema::image_from_job_defaults().nwlayers,
        output_mode = PyOutputMode::from(schema::image_from_job_defaults().output_mode),
        pixel_scale = schema::image_from_job_defaults().pixel_scale,
        pol = PyPolarization::from(schema::image_from_job_defaults().pol),
        robust = schema::image_from_job_defaults().robust,
        uvw_max = schema::image_from_job_defaults().uvw_max,
        uvw_min = schema::image_from_job_defaults().uvw_min,
        weighting = PyWeighting::from(schema::image_from_job_defaults().weighting),
        wstack_nwlayers = schema::image_from_job_defaults().wstack_nwlayers,
        allow_resubmit = schema::image_from_job_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_image_from_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        source_job_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        apply_primary_beam: bool,
        auto_mask: i64,
        auto_threshold: f64,
        abs_threshold: Option<f64>,
        channels_out: i64,
        clean_iterations: i64,
        clean_threshold: Option<f64>,
        image_size: i64,
        join_channels: bool,
        join_polarizations: bool,
        mgain: f64,
        multiscale: bool,
        nmiter: u64,
        nwlayers: Option<i64>,
        output_mode: PyOutputMode,
        pixel_scale: f64,
        pol: PyPolarization,
        robust: f64,
        uvw_max: Option<f64>,
        uvw_min: f64,
        weighting: PyWeighting,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let source_job_id = validate::source_job_id(source_job_id).map_err(|e| api_error(py, e))?;
        let image_size = validate::image_size(image_size).map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(nmiter).map_err(|e| api_error(py, e))?;
        let mut params = schema::image_from_job_params(job_obs_id(obs_id)?, source_job_id);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.apply_primary_beam = apply_primary_beam;
        params.auto_mask = auto_mask;
        params.auto_threshold = auto_threshold;
        params.abs_threshold = abs_threshold;
        params.channels_out = channels_out;
        params.clean_iterations = clean_iterations;
        params.clean_threshold = clean_threshold;
        params.image_size = image_size;
        params.join_channels = join_channels;
        params.join_polarizations = join_polarizations;
        params.mgain = mgain;
        params.multiscale = multiscale;
        params.nmiter = nmiter;
        params.nwlayers = nwlayers;
        params.output_mode = output_mode.into();
        params.pixel_scale = pixel_scale;
        params.pol = pol.into();
        params.robust = robust;
        params.uvw_max = uvw_max;
        params.uvw_min = uvw_min;
        params.weighting = weighting.into();
        params.wstack_nwlayers = wstack_nwlayers;
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_image_from_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a job to download an observation's voltage data.
    ///
    /// Every keyword argument has the MWA ASVO's default as its default
    /// value (the type stubs show it). `None` is accepted by the arguments
    /// that the MWA ASVO allows to be `null`, and is sent as `null`: it
    /// does not mean "use the default".
    ///
    /// Args:
    ///     obs_id: The Obs ID.
    ///     offset: The offset in seconds from the start GPS time of the
    ///         observation, from 0 to 5400.
    ///     duration: The duration to download, in seconds.
    ///     delivery: Where the MWA ASVO delivers the data. The only valid
    ///         value is "scratch", which needs the "mwavcs" Pawsey Group
    ///         on your MWA ASVO profile.
    ///     from_channel: The first receiver channel number (0 to 255).
    ///     to_channel: The last receiver channel number (0 to 255).
    ///     allow_resubmit: Submit the job even if an identical one has
    ///         completed.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, or `offset` is not
    ///         from 0 to 5400. Nothing is sent.
    ///     OverflowError: A channel number is not from 0 to 255.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, offset, duration, *, \
        delivery='scratch', \
        from_channel=None, \
        to_channel=None, \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        offset,
        duration,
        *,
        delivery = schema::voltage_defaults().delivery,
        from_channel = schema::voltage_defaults().from_channel,
        to_channel = schema::voltage_defaults().to_channel,
        allow_resubmit = schema::voltage_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_voltage_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        offset: i64,
        duration: u64,
        delivery: String,
        from_channel: Option<u8>,
        to_channel: Option<u8>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let mut params = schema::voltage_params(job_obs_id(obs_id)?, offset, duration);
        params.delivery = delivery;
        params.from_channel = from_channel;
        params.to_channel = to_channel;
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_voltage_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a job to download an observation's beamformer data.
    ///
    /// The arguments are those of `submit_download_vis_job`.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid Obs ID, or `delivery` is
    ///         `Delivery.Acacia` and `delivery_format` is
    ///         `DeliveryFormat.Files` (Acacia delivers a tar only). Nothing
    ///         is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(text_signature = "($self, obs_id, *, \
        delivery='Delivery.Acacia', \
        delivery_format='DeliveryFormat.Tar', \
        allow_resubmit=False\
        )")]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDelivery::from(schema::beamformer_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::beamformer_defaults().delivery_format),
        allow_resubmit = schema::beamformer_defaults().allow_resubmit,
    ))]
    fn submit_beamformer_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: PyDelivery,
        delivery_format: PyDeliveryFormat,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let mut params = schema::beamformer_params(job_obs_id(obs_id)?);
        params.delivery = delivery.into();
        params.delivery_format = delivery_format.into();
        params.allow_resubmit = allow_resubmit;
        py.detach(|| self.inner.submit_beamformer_job(&params))
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
    ///     The server's reply for each job, in the order of the file. An
    ///     empty list if the file has no job.
    ///
    /// Raises:
    ///     OSError: The file cannot be read. Nothing is submitted.
    ///     ValueError: A line of the file has a problem (for example an
    ///         unknown key, or a value outside the limits of the MWA ASVO).
    ///         The message lists every bad line by its line number. Nothing
    ///         is submitted.
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
        delivery: Option<PyDelivery>,
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
    ///     ValueError: `obs_id` is not a valid Obs ID.
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
        obs_id: u64,
        download_dir: PathBuf,
        keep_tar: bool,
        no_resume: bool,
        hash: bool,
        progress: Option<ProgressCallback>,
        buffer_size: Option<usize>,
        retry_duration: Option<f64>,
        download_number: usize,
        download_count: usize,
        jobs: Option<PyRef<'_, PyAsvoJobVec>>,
    ) -> PyResult<PyAsvoJob> {
        let obs_id = job_obs_id(obs_id)?;
        let args = PyDownloadArgs {
            download_dir,
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
