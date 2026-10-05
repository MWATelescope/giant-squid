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
use super::download::{run_download, PyDownloadArgs};
use super::error::api_error;
use super::params::{
    BeamformerArgs, ConversionArgs, DownloadArgs, ImageFromJobArgs, ImagingArgs, VoltageArgs,
};
use super::typed::{JobId, ProgressCallback};
use super::types::{
    PyAsvoJobVec, PyCentre, PyDelivery, PyDeliveryFormat, PyJobCancelledResponse, PyJobState,
    PyJobSubmittedResponse, PyJobType, PyOutput, PyOutputMode, PyPolarization, PyWeighting,
};
use crate::asvo::{AsvoClient, AsvoClientConfig, JobQuery, JobsFilter};
use crate::obs_id::ObsId;

/// A client for the MWA ASVO. It logs in when it is created.
///
/// Args:
///     host: The MWA ASVO base URL, for example
///         "https://asvo.mwatelescope.org:443". An "http://" host is
///         permitted (a local test server); every other host must use TLS.
///     api_key: Your MWA ASVO API key.
///     api_timeout: The timeout for one API request, in seconds. `None`
///         uses the library default (60 s).
///     token_cache_path: Where to cache the session between runs. `None`
///         keeps the session in memory only, so every new client logs in.
///         Give a path for a script that runs often, because the server
///         permits only a few logins a minute.
///
/// Raises:
///     AsvoApiError: The API key is empty, or the login failed.
///     ValueError: `api_timeout` is negative or not finite.
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
            .map_err(|e| api_error(py, e))?;
        Ok(Self { inner, host })
    }

    /// Get your jobs. The server filters them; every filter that is `None`
    /// does not filter.
    ///
    /// Args:
    ///     days: Only the jobs from the past `days` days, from 1 to 30.
    ///         `None` is the default of the MWA ASVO API.
    ///     job_state: Only the jobs in this state. The server takes one
    ///         state; to filter by several, use `AsvoJobVec.filter`.
    ///     job_type: Only the jobs of this type.
    ///     date_from: Only the jobs created at or after this time. It must
    ///         have a time zone.
    ///     date_to: Only the jobs created at or before this time. It must
    ///         have a time zone.
    ///     sort_by: The column to sort the jobs by, for example "id".
    ///         `None` uses the server's default order.
    ///
    /// Raises:
    ///     ValueError: `days` is not from 1 to 30.
    ///     TypeError: `date_from` or `date_to` has no time zone.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (days=None, *, job_state=None, job_type=None, date_from=None, date_to=None, sort_by=None))]
    #[allow(clippy::too_many_arguments)]
    fn get_jobs(
        &self,
        py: Python<'_>,
        days: Option<i64>,
        job_state: Option<PyJobState>,
        job_type: Option<PyJobType>,
        date_from: Option<Timestamp>,
        date_to: Option<Timestamp>,
        sort_by: Option<String>,
    ) -> PyResult<PyAsvoJobVec> {
        let filter = JobsFilter {
            days,
            job_state: job_state.map(Into::into),
            job_type: job_type.map(Into::into),
            date_from,
            date_to,
            sort_by,
        };
        py.detach(|| self.inner.get_jobs(&filter))
            .map(PyAsvoJobVec::from)
            .map_err(|e| api_error(py, e))
    }

    /// List jobs as `giant-squid list` does: the server filters what it
    /// can, and the lists (several job IDs, obsids, types or states) are
    /// applied to the result. Every filter that is `None` does not filter.
    ///
    /// Args:
    ///     job_ids: Only these jobs. Cannot be combined with `obs_ids`.
    ///     obs_ids: Only the jobs for these obsids.
    ///     job_types: Only the jobs of these types.
    ///     job_states: Only the jobs in these states.
    ///     days: Only the jobs from the past `days` days, from 1 to 30.
    ///         `None` is the default of the MWA ASVO API.
    ///     date_from: Only the jobs created at or after this time. It must
    ///         have a time zone.
    ///     date_to: Only the jobs created at or before this time. It must
    ///         have a time zone.
    ///     sort_by: The column to sort the jobs by, for example "id".
    ///
    /// Raises:
    ///     ValueError: Both `job_ids` and `obs_ids` are given, an obsid is
    ///         not valid, or `days` is not from 1 to 30.
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
    fn list_jobs(
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
            .map(|o| ObsId::validate(o).map_err(|e| PyValueError::new_err(e.to_string())))
            .collect::<PyResult<Vec<ObsId>>>()?;
        let query = JobQuery {
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
            days,
            date_from,
            date_to,
            sort_by,
        };
        py.detach(|| self.inner.list_jobs(&query))
            .map(PyAsvoJobVec::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a job to download an observation's raw visibilities.
    ///
    /// Every keyword argument that is `None` uses the MWA ASVO's default.
    ///
    /// Args:
    ///     obs_id: The obsid.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files.
    ///     allow_resubmit: Submit the job even if an identical one has
    ///         completed.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid obsid.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
    fn submit_download_vis_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = DownloadArgs {
            delivery,
            delivery_format,
            allow_resubmit,
        }
        .into_params(obs_id)?;
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
    ///     ValueError: `obs_id` is not a valid obsid.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
    fn submit_download_meta_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = DownloadArgs {
            delivery,
            delivery_format,
            allow_resubmit,
        }
        .into_params(obs_id)?;
        py.detach(|| self.inner.submit_download_meta_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a conversion (preprocessing) job.
    ///
    /// Every keyword argument that is `None` uses the MWA ASVO's default.
    ///
    /// Args:
    ///     obs_id: The obsid.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files.
    ///     output: The output format.
    ///     avg_freq_res: The frequency resolution to average to (kHz).
    ///     avg_time_res: The time resolution to average to (s).
    ///     flag_edge_width: The width of the frequency edge flagging (kHz).
    ///     apply_di_cal: Apply the DI calibration solution.
    ///     centre: The phase centre mode. `Centre.Custom` needs
    ///         `custom_centre_ra` and `custom_centre_dec`.
    ///     custom_centre_ra: The custom phase centre's right ascension
    ///         (degrees).
    ///     custom_centre_dec: The custom phase centre's declination
    ///         (degrees).
    ///     no_apply_amps: Do not apply the amplitude calibration solutions.
    ///     no_digital_gains: Do not apply the digital gains.
    ///     no_flag_dc: Do not flag the DC channel.
    ///     no_geometry_delay: Do not apply the geometric delay corrections.
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
    ///     ValueError: `obs_id` is not a valid obsid, or a number is outside
    ///         what the MWA ASVO accepts (for example `avg_freq_res` above
    ///         1280). The message names the argument and its limits.
    ///         Nothing is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (
        obs_id,
        *,
        delivery=None,
        delivery_format=None,
        output=None,
        avg_freq_res=None,
        avg_time_res=None,
        flag_edge_width=None,
        apply_di_cal=None,
        centre=None,
        custom_centre_ra=None,
        custom_centre_dec=None,
        no_apply_amps=None,
        no_digital_gains=None,
        no_flag_dc=None,
        no_geometry_delay=None,
        no_passband_gains=None,
        no_cable_delay=None,
        no_rfi=None,
        allow_resubmit=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_conversion_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        output: Option<PyOutput>,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        flag_edge_width: Option<f64>,
        apply_di_cal: Option<bool>,
        centre: Option<PyCentre>,
        custom_centre_ra: Option<f64>,
        custom_centre_dec: Option<f64>,
        no_apply_amps: Option<bool>,
        no_digital_gains: Option<bool>,
        no_flag_dc: Option<bool>,
        no_geometry_delay: Option<bool>,
        no_passband_gains: Option<bool>,
        no_cable_delay: Option<bool>,
        no_rfi: Option<bool>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = ConversionArgs {
            delivery,
            delivery_format,
            output,
            avg_freq_res,
            avg_time_res,
            flag_edge_width,
            apply_di_cal,
            centre,
            custom_centre_ra,
            custom_centre_dec,
            no_apply_amps,
            no_digital_gains,
            no_flag_dc,
            no_geometry_delay,
            no_passband_gains,
            no_cable_delay,
            no_rfi,
            allow_resubmit,
        }
        .into_params(obs_id)?;
        py.detach(|| self.inner.submit_conversion_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit an imaging job that starts from an obsid.
    ///
    /// Every keyword argument that is `None` uses the MWA ASVO's default.
    ///
    /// Args:
    ///     obs_id: The obsid.
    ///     delivery: Where the MWA ASVO delivers the data.
    ///     delivery_format: How the MWA ASVO packages the files.
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
    ///         `custom_centre_ra` and `custom_centre_dec`.
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
    ///     no_geometry_delay: Do not apply the geometric delay corrections.
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
    ///     ValueError: `obs_id` is not a valid obsid, or an argument is
    ///         outside what the MWA ASVO accepts (for example `mgain`
    ///         above 1, or an `image_size` that is not a supported size).
    ///         The message names the argument and its limits. Nothing is
    ///         sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (
        obs_id,
        *,
        delivery=None,
        delivery_format=None,
        apply_di_cal=None,
        apply_primary_beam=None,
        auto_mask=None,
        auto_threshold=None,
        abs_threshold=None,
        avg_freq_res=None,
        avg_time_res=None,
        channels_out=None,
        clean_iterations=None,
        clean_threshold=None,
        centre=None,
        custom_centre_dec=None,
        custom_centre_ra=None,
        flag_edge_width=None,
        image_size=None,
        join_channels=None,
        join_polarizations=None,
        mgain=None,
        multiscale=None,
        nmiter=None,
        no_apply_amps=None,
        no_digital_gains=None,
        no_flag_dc=None,
        no_geometry_delay=None,
        no_passband_gains=None,
        no_cable_delay=None,
        no_rfi=None,
        nwlayers=None,
        output_mode=None,
        pixel_scale=None,
        pol=None,
        robust=None,
        uvw_max=None,
        uvw_min=None,
        weighting=None,
        wstack_nwlayers=None,
        allow_resubmit=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_imaging_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        apply_di_cal: Option<bool>,
        apply_primary_beam: Option<bool>,
        auto_mask: Option<i64>,
        auto_threshold: Option<f64>,
        abs_threshold: Option<f64>,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        channels_out: Option<i64>,
        clean_iterations: Option<i64>,
        clean_threshold: Option<f64>,
        centre: Option<PyCentre>,
        custom_centre_dec: Option<f64>,
        custom_centre_ra: Option<f64>,
        flag_edge_width: Option<f64>,
        image_size: Option<i64>,
        join_channels: Option<bool>,
        join_polarizations: Option<bool>,
        mgain: Option<f64>,
        multiscale: Option<bool>,
        nmiter: Option<u64>,
        no_apply_amps: Option<bool>,
        no_digital_gains: Option<bool>,
        no_flag_dc: Option<bool>,
        no_geometry_delay: Option<bool>,
        no_passband_gains: Option<bool>,
        no_cable_delay: Option<bool>,
        no_rfi: Option<bool>,
        nwlayers: Option<i64>,
        output_mode: Option<PyOutputMode>,
        pixel_scale: Option<f64>,
        pol: Option<PyPolarization>,
        robust: Option<f64>,
        uvw_max: Option<f64>,
        uvw_min: Option<f64>,
        weighting: Option<PyWeighting>,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = ImagingArgs {
            delivery,
            delivery_format,
            apply_di_cal,
            apply_primary_beam,
            auto_mask,
            auto_threshold,
            abs_threshold,
            avg_freq_res,
            avg_time_res,
            channels_out,
            clean_iterations,
            clean_threshold,
            centre,
            custom_centre_dec,
            custom_centre_ra,
            flag_edge_width,
            image_size,
            join_channels,
            join_polarizations,
            mgain,
            multiscale,
            nmiter,
            no_apply_amps,
            no_digital_gains,
            no_flag_dc,
            no_geometry_delay,
            no_passband_gains,
            no_cable_delay,
            no_rfi,
            nwlayers,
            output_mode,
            pixel_scale,
            pol,
            robust,
            uvw_max,
            uvw_min,
            weighting,
            wstack_nwlayers,
            allow_resubmit,
        }
        .into_params(obs_id)?;
        py.detach(|| self.inner.submit_imaging_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit an imaging job that starts from an existing conversion job.
    ///
    /// Every keyword argument that is `None` uses the MWA ASVO's default.
    /// The arguments are those of `submit_imaging_job`, except that the
    /// data come from a conversion job, so there are no calibration,
    /// averaging, flagging or phase centre arguments.
    ///
    /// Args:
    ///     obs_id: The obsid.
    ///     source_job_id: The ID of the conversion job to image. Must be
    ///         greater than zero.
    ///
    /// Returns:
    ///     The server's reply, with the new job's ID.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid obsid, `source_job_id` is
    ///         zero, or an argument is outside what the MWA ASVO accepts
    ///         (see `submit_imaging_job`). Nothing is sent.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (
        obs_id,
        source_job_id,
        *,
        delivery=None,
        delivery_format=None,
        apply_primary_beam=None,
        auto_mask=None,
        auto_threshold=None,
        abs_threshold=None,
        channels_out=None,
        clean_iterations=None,
        clean_threshold=None,
        image_size=None,
        join_channels=None,
        join_polarizations=None,
        mgain=None,
        multiscale=None,
        nmiter=None,
        nwlayers=None,
        output_mode=None,
        pixel_scale=None,
        pol=None,
        robust=None,
        uvw_max=None,
        uvw_min=None,
        weighting=None,
        wstack_nwlayers=None,
        allow_resubmit=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_image_from_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        source_job_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        apply_primary_beam: Option<bool>,
        auto_mask: Option<i64>,
        auto_threshold: Option<f64>,
        abs_threshold: Option<f64>,
        channels_out: Option<i64>,
        clean_iterations: Option<i64>,
        clean_threshold: Option<f64>,
        image_size: Option<i64>,
        join_channels: Option<bool>,
        join_polarizations: Option<bool>,
        mgain: Option<f64>,
        multiscale: Option<bool>,
        nmiter: Option<u64>,
        nwlayers: Option<i64>,
        output_mode: Option<PyOutputMode>,
        pixel_scale: Option<f64>,
        pol: Option<PyPolarization>,
        robust: Option<f64>,
        uvw_max: Option<f64>,
        uvw_min: Option<f64>,
        weighting: Option<PyWeighting>,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = ImageFromJobArgs {
            delivery,
            delivery_format,
            apply_primary_beam,
            auto_mask,
            auto_threshold,
            abs_threshold,
            channels_out,
            clean_iterations,
            clean_threshold,
            image_size,
            join_channels,
            join_polarizations,
            mgain,
            multiscale,
            nmiter,
            nwlayers,
            output_mode,
            pixel_scale,
            pol,
            robust,
            uvw_max,
            uvw_min,
            weighting,
            wstack_nwlayers,
            allow_resubmit,
        }
        .into_params(obs_id, source_job_id)?;
        py.detach(|| self.inner.submit_image_from_job(&params))
            .map(PyJobSubmittedResponse::from)
            .map_err(|e| api_error(py, e))
    }

    /// Submit a job to download an observation's voltage data.
    ///
    /// Every keyword argument that is `None` uses the MWA ASVO's default.
    ///
    /// Args:
    ///     obs_id: The obsid.
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
    ///     ValueError: `obs_id` is not a valid obsid, or `offset` is not
    ///         from 0 to 5400. Nothing is sent.
    ///     OverflowError: A channel number is not from 0 to 255.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (
        obs_id,
        offset,
        duration,
        *,
        delivery=None,
        from_channel=None,
        to_channel=None,
        allow_resubmit=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn submit_voltage_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        offset: i64,
        duration: u64,
        delivery: Option<String>,
        from_channel: Option<u8>,
        to_channel: Option<u8>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = VoltageArgs {
            offset,
            duration,
            delivery,
            from_channel,
            to_channel,
            allow_resubmit,
        }
        .into_params(obs_id)?;
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
    ///     ValueError: `obs_id` is not a valid obsid.
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (obs_id, *, delivery=None, delivery_format=None, allow_resubmit=None))]
    fn submit_beamformer_job(
        &self,
        py: Python<'_>,
        obs_id: u64,
        delivery: Option<PyDelivery>,
        delivery_format: Option<PyDeliveryFormat>,
        allow_resubmit: Option<bool>,
    ) -> PyResult<PyJobSubmittedResponse> {
        let params = BeamformerArgs {
            delivery,
            delivery_format,
            allow_resubmit,
        }
        .into_params(obs_id)?;
        py.detach(|| self.inner.submit_beamformer_job(&params))
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
    ///     OverflowError: `job_id` is negative or too large to be a job ID.
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
    ///     job_id: The job ID.
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
    ///         written. `None` uses the library default (100 MiB).
    ///     retry_duration: How long to retry a failing download, in
    ///         seconds. 0 disables retries. `None` uses the library default
    ///         (900 s).
    ///     download_number: The number of this download, in a series,
    ///         for the progress and log label (`[1/2]`).
    ///     download_count: How many downloads there are in the series.
    ///
    /// Raises:
    ///     AsvoError: The job is missing, not ready or has no files, a
    ///         transfer failed, or the hash does not match.
    ///     AsvoApiError: Getting the job list failed.
    ///     ValueError: `download_dir` or `retry_duration` is not valid.
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
    ) -> PyResult<()> {
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
        run_download(py, args, |opts| self.inner.download_job(job_id.0, opts))
    }

    /// Download the files of the one ready job for an obsid.
    ///
    /// The arguments, and the way the download runs, are those of
    /// `download_job`.
    ///
    /// Args:
    ///     obs_id: The obsid. There must be exactly one ready job for it.
    ///
    /// Raises:
    ///     ValueError: `obs_id` is not a valid obsid.
    ///     AsvoError: No job, no ready job, or more than one ready job has
    ///         this obsid, or the download failed (see `download_job`).
    ///     AsvoApiError: Getting the job list failed.
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
    ) -> PyResult<()> {
        let obs_id = ObsId::validate(obs_id).map_err(|e| PyValueError::new_err(e.to_string()))?;
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
        run_download(py, args, |opts| self.inner.download_obs(obs_id, opts))
    }

    fn __repr__(&self) -> String {
        format!("AsvoClient(host={:?})", self.host)
    }
}
