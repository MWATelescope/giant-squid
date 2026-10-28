// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The request bodies of the submit methods, as Python classes.
//!
//! Each class is the schema type of one job request body
//! (`DownloadJobParams`, `ConversionJobParams`, `ImagingJobFlow1Params`,
//! `ImagingJobFlow2Params`), with its name and field names. The arguments
//! of the constructor have the defaults that the library's `job_params`
//! gives from the OpenAPI schema, and each argument is a plain attribute
//! that can be read and changed.
//!
//! A class holds Python values. It checks nothing when it is made or
//! changed: the submit method turns it into the library's request body,
//! which checks it, and a value that the schema does not allow raises
//! `ValueError` (see `super::error::api_error`) before anything is sent.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use super::error::api_error;
use super::obs_id::RawObsId;
use super::params::job_obs_id;
use super::types::{
    PyDeliveryDestination, PyDeliveryFormat, PyImageOutputMode, PyImageWeighting, PyOutputFormat,
    PyPhaseCentre, PyPolarization,
};
use crate::mwa_asvo::api::job_params::{self as schema, PreprocessJobParams};
use crate::mwa_asvo::api::openapi::{
    ConversionJobParams, DownloadJobParams, ImagingJobFlow1Params, ImagingJobFlow2Params,
};
use crate::mwa_asvo::api::validate;

/// The request body of a download job: the schema's `DownloadJobParams`. It is the body of a
/// visibility, metadata, voltage or beamformer download job. The body does not say which: the
/// MWA ASVO finds that from the observation, and it refuses a body that does not suit it (for
/// example a voltage job's `offset` for a visibility observation, or a delivery that the kind
/// of observation does not allow).
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// Nothing is checked until the job is submitted (see the submit method).
///
/// Args:
///     obs_id: The Obs ID.
///     delivery: Where the MWA ASVO delivers the data.
///     delivery_format: How the MWA ASVO packages the files. Must be
///         `DeliveryFormat.Tar` when `delivery` is `DeliveryDestination.Acacia`.
///     allow_resubmit: Submit the job even if an identical one has
///         completed.
///     only_get_metadata: Download only the observation's metadata,
///         not its data.
///     offset: For a voltage observation, the offset in seconds from
///         the start GPS time of the observation, from 0 to 5400.
///     duration: For a voltage observation, the duration to download,
///         in seconds.
///     from_channel: For a voltage observation, the first receiver
///         channel number (0 to 255).
///     to_channel: For a voltage observation, the last receiver channel
///         number (0 to 255).
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "DownloadJobParams", module = "mwa_giant_squid")]
pub struct PyDownloadJobParams {
    #[pyo3(get, set)]
    pub obs_id: RawObsId,
    #[pyo3(get, set)]
    pub delivery: PyDeliveryDestination,
    #[pyo3(get, set)]
    pub delivery_format: PyDeliveryFormat,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
    #[pyo3(get, set)]
    pub only_get_metadata: bool,
    #[pyo3(get, set)]
    pub offset: Option<i64>,
    #[pyo3(get, set)]
    pub duration: Option<u64>,
    #[pyo3(get, set)]
    pub from_channel: Option<u8>,
    #[pyo3(get, set)]
    pub to_channel: Option<u8>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyDownloadJobParams {
    #[new]
    #[pyo3(
        text_signature = "(obs_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', allow_resubmit=False, only_get_metadata=False, offset=None, duration=None, from_channel=None, to_channel=None)"
    )]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDeliveryDestination::from(schema::download_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::download_defaults().delivery_format),
        allow_resubmit = schema::download_defaults().allow_resubmit,
        only_get_metadata = schema::download_defaults().only_get_metadata,
        offset = schema::download_defaults().offset,
        duration = schema::download_defaults().duration,
        from_channel = schema::download_defaults().from_channel,
        to_channel = schema::download_defaults().to_channel,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        obs_id: RawObsId,
        delivery: PyDeliveryDestination,
        delivery_format: PyDeliveryFormat,
        allow_resubmit: Option<bool>,
        only_get_metadata: bool,
        offset: Option<i64>,
        duration: Option<u64>,
        from_channel: Option<u8>,
        to_channel: Option<u8>,
    ) -> Self {
        Self {
            obs_id,
            delivery,
            delivery_format,
            allow_resubmit,
            only_get_metadata,
            offset,
            duration,
            from_channel,
            to_channel,
        }
    }
}

impl PyDownloadJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size or `nmiter` is not
    /// valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<DownloadJobParams> {
        let mut params = schema::download_params(job_obs_id(py, self.obs_id)?);
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.allow_resubmit = self.allow_resubmit;
        params.only_get_metadata = self.only_get_metadata;
        params.offset = self.offset;
        params.duration = self.duration;
        params.from_channel = self.from_channel;
        params.to_channel = self.to_channel;
        Ok(params)
    }
}

/// The request body of a conversion (preprocessing) job: the schema's `ConversionJobParams`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// Nothing is checked until the job is submitted (see the submit method).
///
/// Args:
///     obs_id: The Obs ID.
///     delivery: Where the MWA ASVO delivers the data.
///     delivery_format: How the MWA ASVO packages the files. Must be
///         `DeliveryFormat.Tar` when `delivery` is `DeliveryDestination.Acacia`.
///     output: The output format.
///     avg_freq_res: The frequency resolution to average to (kHz).
///     avg_time_res: The time resolution to average to (s).
///     flag_edge_width: The width of the frequency edge flagging (kHz).
///     apply_di_cal: Apply the DI calibration solution.
///     centre: The phase centre mode. `PhaseCentre.Custom` needs
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
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ConversionJobParams", module = "mwa_giant_squid")]
pub struct PyConversionJobParams {
    #[pyo3(get, set)]
    pub obs_id: RawObsId,
    #[pyo3(get, set)]
    pub delivery: PyDeliveryDestination,
    #[pyo3(get, set)]
    pub delivery_format: PyDeliveryFormat,
    #[pyo3(get, set)]
    pub output: PyOutputFormat,
    #[pyo3(get, set)]
    pub avg_freq_res: Option<f64>,
    #[pyo3(get, set)]
    pub avg_time_res: Option<f64>,
    #[pyo3(get, set)]
    pub flag_edge_width: f64,
    #[pyo3(get, set)]
    pub apply_di_cal: bool,
    #[pyo3(get, set)]
    pub centre: PyPhaseCentre,
    #[pyo3(get, set)]
    pub custom_centre_ra: Option<f64>,
    #[pyo3(get, set)]
    pub custom_centre_dec: Option<f64>,
    #[pyo3(get, set)]
    pub no_apply_amps: bool,
    #[pyo3(get, set)]
    pub no_digital_gains: Option<bool>,
    #[pyo3(get, set)]
    pub no_flag_dc: Option<bool>,
    #[pyo3(get, set)]
    pub no_geometric_delay: Option<bool>,
    #[pyo3(get, set)]
    pub no_passband_gains: Option<bool>,
    #[pyo3(get, set)]
    pub no_cable_delay: Option<bool>,
    #[pyo3(get, set)]
    pub no_rfi: Option<bool>,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyConversionJobParams {
    #[new]
    #[pyo3(
        text_signature = "(obs_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', output='OutputFormat.Ms', avg_freq_res=40.0, avg_time_res=2.0, flag_edge_width=80.0, apply_di_cal=False, centre='PhaseCentre.Phase', custom_centre_ra=None, custom_centre_dec=None, no_apply_amps=False, no_digital_gains=False, no_flag_dc=False, no_geometric_delay=False, no_passband_gains=False, no_cable_delay=False, no_rfi=False, allow_resubmit=False)"
    )]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDeliveryDestination::from(schema::conversion_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::conversion_defaults().delivery_format),
        output = PyOutputFormat::from(schema::conversion_defaults().output),
        avg_freq_res = schema::conversion_defaults().avg_freq_res,
        avg_time_res = schema::conversion_defaults().avg_time_res,
        flag_edge_width = schema::conversion_defaults().flag_edge_width,
        apply_di_cal = schema::conversion_defaults().apply_di_cal,
        centre = PyPhaseCentre::from(schema::conversion_defaults().centre),
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
    fn new(
        obs_id: RawObsId,
        delivery: PyDeliveryDestination,
        delivery_format: PyDeliveryFormat,
        output: PyOutputFormat,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        flag_edge_width: f64,
        apply_di_cal: bool,
        centre: PyPhaseCentre,
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
    ) -> Self {
        Self {
            obs_id,
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
            no_geometric_delay,
            no_passband_gains,
            no_cable_delay,
            no_rfi,
            allow_resubmit,
        }
    }
}

impl PyConversionJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size or `nmiter` is not
    /// valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<ConversionJobParams> {
        let mut params = schema::conversion_params(job_obs_id(py, self.obs_id)?);
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.output = self.output.into();
        params.avg_freq_res = self.avg_freq_res;
        params.avg_time_res = self.avg_time_res;
        params.flag_edge_width = self.flag_edge_width;
        params.apply_di_cal = self.apply_di_cal;
        params.centre = self.centre.into();
        params.custom_centre_ra = self.custom_centre_ra;
        params.custom_centre_dec = self.custom_centre_dec;
        params.no_apply_amps = self.no_apply_amps;
        params.no_digital_gains = self.no_digital_gains;
        params.no_flag_dc = self.no_flag_dc;
        params.no_geometric_delay = self.no_geometric_delay;
        params.no_passband_gains = self.no_passband_gains;
        params.no_cable_delay = self.no_cable_delay;
        params.no_rfi = self.no_rfi;
        params.allow_resubmit = self.allow_resubmit;
        Ok(params)
    }
}

/// The request body of an imaging job that starts from an Obs ID (flow 1): the schema's
/// `ImagingJobFlow1Params`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// Nothing is checked until the job is submitted (see the submit method).
///
/// Args:
///     obs_id: The Obs ID.
///     delivery: Where the MWA ASVO delivers the data.
///     delivery_format: How the MWA ASVO packages the files. Must be
///         `DeliveryFormat.Tar` when `delivery` is `DeliveryDestination.Acacia`.
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
///     centre: Where to centre the image. `PhaseCentre.Custom` needs
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
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ImagingJobFlow1Params", module = "mwa_giant_squid")]
pub struct PyImagingJobFlow1Params {
    #[pyo3(get, set)]
    pub obs_id: RawObsId,
    #[pyo3(get, set)]
    pub delivery: PyDeliveryDestination,
    #[pyo3(get, set)]
    pub delivery_format: PyDeliveryFormat,
    #[pyo3(get, set)]
    pub apply_di_cal: bool,
    #[pyo3(get, set)]
    pub apply_primary_beam: bool,
    #[pyo3(get, set)]
    pub auto_mask: i64,
    #[pyo3(get, set)]
    pub auto_threshold: f64,
    #[pyo3(get, set)]
    pub abs_threshold: Option<f64>,
    #[pyo3(get, set)]
    pub avg_freq_res: Option<f64>,
    #[pyo3(get, set)]
    pub avg_time_res: Option<f64>,
    #[pyo3(get, set)]
    pub channels_out: i64,
    #[pyo3(get, set)]
    pub clean_iterations: i64,
    #[pyo3(get, set)]
    pub clean_threshold: Option<f64>,
    #[pyo3(get, set)]
    pub centre: PyPhaseCentre,
    #[pyo3(get, set)]
    pub custom_centre_dec: Option<f64>,
    #[pyo3(get, set)]
    pub custom_centre_ra: Option<f64>,
    #[pyo3(get, set)]
    pub flag_edge_width: f64,
    #[pyo3(get, set)]
    pub image_size: i64,
    #[pyo3(get, set)]
    pub join_channels: bool,
    #[pyo3(get, set)]
    pub join_polarizations: bool,
    #[pyo3(get, set)]
    pub mgain: f64,
    #[pyo3(get, set)]
    pub multiscale: bool,
    #[pyo3(get, set)]
    pub nmiter: u64,
    #[pyo3(get, set)]
    pub no_apply_amps: bool,
    #[pyo3(get, set)]
    pub no_digital_gains: Option<bool>,
    #[pyo3(get, set)]
    pub no_flag_dc: Option<bool>,
    #[pyo3(get, set)]
    pub no_geometric_delay: Option<bool>,
    #[pyo3(get, set)]
    pub no_passband_gains: Option<bool>,
    #[pyo3(get, set)]
    pub no_cable_delay: Option<bool>,
    #[pyo3(get, set)]
    pub no_rfi: Option<bool>,
    #[pyo3(get, set)]
    pub nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub output_mode: PyImageOutputMode,
    #[pyo3(get, set)]
    pub pixel_scale: f64,
    #[pyo3(get, set)]
    pub pol: PyPolarization,
    #[pyo3(get, set)]
    pub robust: f64,
    #[pyo3(get, set)]
    pub uvw_max: Option<f64>,
    #[pyo3(get, set)]
    pub uvw_min: f64,
    #[pyo3(get, set)]
    pub weighting: PyImageWeighting,
    #[pyo3(get, set)]
    pub wstack_nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyImagingJobFlow1Params {
    #[new]
    #[pyo3(
        text_signature = "(obs_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', apply_di_cal=True, apply_primary_beam=True, auto_mask=3, auto_threshold=0.5, abs_threshold=0.001, avg_freq_res=40.0, avg_time_res=2.0, channels_out=4, clean_iterations=100000, clean_threshold=0.001, centre='PhaseCentre.Phase', custom_centre_dec=None, custom_centre_ra=None, flag_edge_width=80.0, image_size=3072, join_channels=True, join_polarizations=False, mgain=0.8, multiscale=False, nmiter=10, no_apply_amps=False, no_digital_gains=False, no_flag_dc=False, no_geometric_delay=False, no_passband_gains=False, no_cable_delay=False, no_rfi=False, nwlayers=None, output_mode='ImageOutputMode.Fits', pixel_scale=20.0, pol='Polarization.Xxyy', robust=-0.5, uvw_max=None, uvw_min=75.0, weighting='ImageWeighting.Briggs', wstack_nwlayers=None, allow_resubmit=False)"
    )]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDeliveryDestination::from(schema::imaging_defaults().delivery),
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
        centre = PyPhaseCentre::from(schema::imaging_defaults().centre),
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
        output_mode = PyImageOutputMode::from(schema::imaging_defaults().output_mode),
        pixel_scale = schema::imaging_defaults().pixel_scale,
        pol = PyPolarization::from(schema::imaging_defaults().pol),
        robust = schema::imaging_defaults().robust,
        uvw_max = schema::imaging_defaults().uvw_max,
        uvw_min = schema::imaging_defaults().uvw_min,
        weighting = PyImageWeighting::from(schema::imaging_defaults().weighting),
        wstack_nwlayers = schema::imaging_defaults().wstack_nwlayers,
        allow_resubmit = schema::imaging_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        obs_id: RawObsId,
        delivery: PyDeliveryDestination,
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
        centre: PyPhaseCentre,
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
        output_mode: PyImageOutputMode,
        pixel_scale: f64,
        pol: PyPolarization,
        robust: f64,
        uvw_max: Option<f64>,
        uvw_min: f64,
        weighting: PyImageWeighting,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> Self {
        Self {
            obs_id,
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
            no_geometric_delay,
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
    }
}

impl PyImagingJobFlow1Params {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size or `nmiter` is not
    /// valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<ImagingJobFlow1Params> {
        let image_size = validate::image_size(self.image_size).map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(self.nmiter).map_err(|e| api_error(py, e))?;
        let mut params = schema::imaging_params(job_obs_id(py, self.obs_id)?);
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.apply_di_cal = self.apply_di_cal;
        params.apply_primary_beam = self.apply_primary_beam;
        params.auto_mask = self.auto_mask;
        params.auto_threshold = self.auto_threshold;
        params.abs_threshold = self.abs_threshold;
        params.avg_freq_res = self.avg_freq_res;
        params.avg_time_res = self.avg_time_res;
        params.channels_out = self.channels_out;
        params.clean_iterations = self.clean_iterations;
        params.clean_threshold = self.clean_threshold;
        params.centre = self.centre.into();
        params.custom_centre_dec = self.custom_centre_dec;
        params.custom_centre_ra = self.custom_centre_ra;
        params.flag_edge_width = self.flag_edge_width;
        params.image_size = image_size;
        params.join_channels = self.join_channels;
        params.join_polarizations = self.join_polarizations;
        params.mgain = self.mgain;
        params.multiscale = self.multiscale;
        params.nmiter = nmiter;
        params.no_apply_amps = self.no_apply_amps;
        params.no_digital_gains = self.no_digital_gains;
        params.no_flag_dc = self.no_flag_dc;
        params.no_geometric_delay = self.no_geometric_delay;
        params.no_passband_gains = self.no_passband_gains;
        params.no_cable_delay = self.no_cable_delay;
        params.no_rfi = self.no_rfi;
        params.nwlayers = self.nwlayers;
        params.output_mode = self.output_mode.into();
        params.pixel_scale = self.pixel_scale;
        params.pol = self.pol.into();
        params.robust = self.robust;
        params.uvw_max = self.uvw_max;
        params.uvw_min = self.uvw_min;
        params.weighting = self.weighting.into();
        params.wstack_nwlayers = self.wstack_nwlayers;
        params.allow_resubmit = self.allow_resubmit;
        Ok(params)
    }
}

/// The request body of an imaging job that starts from a conversion job (flow 2): the
/// schema's `ImagingJobFlow2Params`. Its `obs_id` is not an argument: the MWA ASVO takes
/// the Obs ID from the source job, so it is sent as `null`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// Nothing is checked until the job is submitted (see the submit method).
///
/// Args:
///     source_job_id: The ID of the conversion job to image. Must be
///         greater than zero.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ImagingJobFlow2Params", module = "mwa_giant_squid")]
pub struct PyImagingJobFlow2Params {
    #[pyo3(get, set)]
    pub source_job_id: u64,
    #[pyo3(get, set)]
    pub delivery: PyDeliveryDestination,
    #[pyo3(get, set)]
    pub delivery_format: PyDeliveryFormat,
    #[pyo3(get, set)]
    pub apply_primary_beam: bool,
    #[pyo3(get, set)]
    pub auto_mask: i64,
    #[pyo3(get, set)]
    pub auto_threshold: f64,
    #[pyo3(get, set)]
    pub abs_threshold: Option<f64>,
    #[pyo3(get, set)]
    pub channels_out: i64,
    #[pyo3(get, set)]
    pub clean_iterations: i64,
    #[pyo3(get, set)]
    pub clean_threshold: Option<f64>,
    #[pyo3(get, set)]
    pub image_size: i64,
    #[pyo3(get, set)]
    pub join_channels: bool,
    #[pyo3(get, set)]
    pub join_polarizations: bool,
    #[pyo3(get, set)]
    pub mgain: f64,
    #[pyo3(get, set)]
    pub multiscale: bool,
    #[pyo3(get, set)]
    pub nmiter: u64,
    #[pyo3(get, set)]
    pub nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub output_mode: PyImageOutputMode,
    #[pyo3(get, set)]
    pub pixel_scale: f64,
    #[pyo3(get, set)]
    pub pol: PyPolarization,
    #[pyo3(get, set)]
    pub robust: f64,
    #[pyo3(get, set)]
    pub uvw_max: Option<f64>,
    #[pyo3(get, set)]
    pub uvw_min: f64,
    #[pyo3(get, set)]
    pub weighting: PyImageWeighting,
    #[pyo3(get, set)]
    pub wstack_nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyImagingJobFlow2Params {
    #[new]
    #[pyo3(
        text_signature = "(source_job_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', apply_primary_beam=True, auto_mask=3, auto_threshold=0.5, abs_threshold=0.001, channels_out=4, clean_iterations=100000, clean_threshold=0.001, image_size=3072, join_channels=True, join_polarizations=False, mgain=0.8, multiscale=False, nmiter=10, nwlayers=None, output_mode='ImageOutputMode.Fits', pixel_scale=20.0, pol='Polarization.Xxyy', robust=-0.5, uvw_max=None, uvw_min=75.0, weighting='ImageWeighting.Briggs', wstack_nwlayers=None, allow_resubmit=False)"
    )]
    #[pyo3(signature = (
        source_job_id,
        *,
        delivery = PyDeliveryDestination::from(schema::image_from_job_defaults().delivery),
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
        output_mode = PyImageOutputMode::from(schema::image_from_job_defaults().output_mode),
        pixel_scale = schema::image_from_job_defaults().pixel_scale,
        pol = PyPolarization::from(schema::image_from_job_defaults().pol),
        robust = schema::image_from_job_defaults().robust,
        uvw_max = schema::image_from_job_defaults().uvw_max,
        uvw_min = schema::image_from_job_defaults().uvw_min,
        weighting = PyImageWeighting::from(schema::image_from_job_defaults().weighting),
        wstack_nwlayers = schema::image_from_job_defaults().wstack_nwlayers,
        allow_resubmit = schema::image_from_job_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        source_job_id: u64,
        delivery: PyDeliveryDestination,
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
        output_mode: PyImageOutputMode,
        pixel_scale: f64,
        pol: PyPolarization,
        robust: f64,
        uvw_max: Option<f64>,
        uvw_min: f64,
        weighting: PyImageWeighting,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> Self {
        Self {
            source_job_id,
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
    }
}

impl PyImagingJobFlow2Params {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size or `nmiter` is not
    /// valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<ImagingJobFlow2Params> {
        let image_size = validate::image_size(self.image_size).map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(self.nmiter).map_err(|e| api_error(py, e))?;
        let mut params = schema::image_from_job_params(
            validate::source_job_id(self.source_job_id).map_err(|e| api_error(py, e))?,
        );
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.apply_primary_beam = self.apply_primary_beam;
        params.auto_mask = self.auto_mask;
        params.auto_threshold = self.auto_threshold;
        params.abs_threshold = self.abs_threshold;
        params.channels_out = self.channels_out;
        params.clean_iterations = self.clean_iterations;
        params.clean_threshold = self.clean_threshold;
        params.image_size = image_size;
        params.join_channels = self.join_channels;
        params.join_polarizations = self.join_polarizations;
        params.mgain = self.mgain;
        params.multiscale = self.multiscale;
        params.nmiter = nmiter;
        params.nwlayers = self.nwlayers;
        params.output_mode = self.output_mode.into();
        params.pixel_scale = self.pixel_scale;
        params.pol = self.pol.into();
        params.robust = self.robust;
        params.uvw_max = self.uvw_max;
        params.uvw_min = self.uvw_min;
        params.weighting = self.weighting.into();
        params.wstack_nwlayers = self.wstack_nwlayers;
        params.allow_resubmit = self.allow_resubmit;
        Ok(params)
    }
}

/// The `params` argument of `submit_preprocess_job`: one of the three
/// preprocess classes, already turned into the library's request body.
pub struct PreprocessParams(pub PreprocessJobParams);

impl<'a, 'py> FromPyObject<'a, 'py> for PreprocessParams {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let py = obj.py();
        if let Ok(params) = obj.cast::<PyConversionJobParams>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        if let Ok(params) = obj.cast::<PyImagingJobFlow1Params>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        if let Ok(params) = obj.cast::<PyImagingJobFlow2Params>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        Err(PyTypeError::new_err(format!(
            "expected a ConversionJobParams, an ImagingJobFlow1Params or an \
             ImagingJobFlow2Params, not {}",
            obj.get_type().name()?
        )))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for PreprocessParams {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo {
            name: "ConversionJobParams | ImagingJobFlow1Params | ImagingJobFlow2Params".to_string(),
            source_module: None,
            import: Default::default(),
            type_refs: std::collections::HashMap::new(),
        }
    }
}
