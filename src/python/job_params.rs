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
//! A class holds Python values. Only the values whose Rust type cannot hold
//! a wrong one are checked when they are set: an Obs ID is an `ObsId` (a
//! number that is not valid raises `ObsIdError`), and `source_job_id` is not
//! 0 (`ValueError`). The other values are checked by `validate()`, and by the
//! submit method, which turns the class into the library's request body, and
//! a value that the schema does not allow raises `ValueError` (see
//! `super::error::api_error`) before anything is sent.
//!
//! The functions `download_params`, `metadata_params`, `voltage_params`,
//! `conversion_params`, `imaging_params` and `image_from_job_params` are the
//! library's functions of the same names: they make the class for the body
//! that the library makes. [`PyPreprocessJobParams`] is the library's
//! `PreprocessJobParams`.

use std::num::NonZeroU64;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use super::error::api_error;
use super::obs_id::{ObsIdArg, PyObsId};
use super::typed::JobId;
use super::types::{
    PyDeliveryDestination, PyDeliveryFormat, PyImageOutputMode, PyImageWeighting, PyOutputFormat,
    PyPhaseCentre, PyPolarization,
};
use crate::mwa_asvo::api::job_params::{self as schema, PreprocessJobParams};
use crate::mwa_asvo::api::openapi::{
    ConversionJobParams, DownloadJobParams, ImagingJobFlow1Params, ImagingJobFlow2Params,
};
use crate::mwa_asvo::api::validate;
use crate::obs_id::ObsId;

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
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
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
    pub obs_id: ObsId,
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
    /// The Obs ID. An `int` or an `ObsId` is accepted, and checked when it is set
    /// (`ObsIdError` if it is not valid).
    #[getter]
    fn obs_id(&self) -> PyObsId {
        PyObsId(self.obs_id)
    }

    #[setter]
    fn set_obs_id(&mut self, obs_id: ObsIdArg) {
        self.obs_id = obs_id.0;
    }

    /// Check the values against the limits of the MWA ASVO, with the library's `validate_download_params`. The submit
    /// methods do this before they send a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        validate::validate_download_params(&self.to_schema()).map_err(|e| api_error(py, e))
    }

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
        obs_id: ObsIdArg,
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
            obs_id: obs_id.0,
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
    pub(super) fn to_schema(&self) -> DownloadJobParams {
        let mut params = schema::download_params(self.obs_id);
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.allow_resubmit = self.allow_resubmit;
        params.only_get_metadata = self.only_get_metadata;
        params.offset = self.offset;
        params.duration = self.duration;
        params.from_channel = self.from_channel;
        params.to_channel = self.to_channel;
        params
    }
}

/// The request body of a conversion (preprocessing) job: the schema's `ConversionJobParams`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
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
    pub obs_id: ObsId,
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
    /// The Obs ID. An `int` or an `ObsId` is accepted, and checked when it is set
    /// (`ObsIdError` if it is not valid).
    #[getter]
    fn obs_id(&self) -> PyObsId {
        PyObsId(self.obs_id)
    }

    #[setter]
    fn set_obs_id(&mut self, obs_id: ObsIdArg) {
        self.obs_id = obs_id.0;
    }

    /// Check the values against the limits of the MWA ASVO, with the library's `validate_conversion_params`. The submit
    /// methods do this before they send a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        validate::validate_conversion_params(&self.to_schema()).map_err(|e| api_error(py, e))
    }

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
        obs_id: ObsIdArg,
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
            obs_id: obs_id.0,
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
    pub(super) fn to_schema(&self) -> ConversionJobParams {
        let mut params = schema::conversion_params(self.obs_id);
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
        params
    }
}

/// The request body of an imaging job that starts from an Obs ID (flow 1): the schema's
/// `ImagingJobFlow1Params`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
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
    pub obs_id: ObsId,
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
    /// The Obs ID. An `int` or an `ObsId` is accepted, and checked when it is set
    /// (`ObsIdError` if it is not valid).
    #[getter]
    fn obs_id(&self) -> PyObsId {
        PyObsId(self.obs_id)
    }

    #[setter]
    fn set_obs_id(&mut self, obs_id: ObsIdArg) {
        self.obs_id = obs_id.0;
    }

    /// Check the values against the limits of the MWA ASVO, with the library's `validate_imaging_params`. The submit
    /// methods do this before they send a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        validate::validate_imaging_params(&self.to_schema(py)?).map_err(|e| api_error(py, e))
    }

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
        obs_id: ObsIdArg,
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
            obs_id: obs_id.0,
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
        let mut params = schema::imaging_params(self.obs_id);
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
/// schema's `ImagingJobFlow2Params`. Its `obs_id` is `None` by default: the MWA ASVO takes
/// the Obs ID from the source job, so it is left out of the request.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
///
/// Args:
///     source_job_id: The ID of the conversion job to image. Must be
///         greater than zero (checked when it is set, `ValueError`).
///     obs_id: The Obs ID, or `None`, which leaves it out of the request:
///         the MWA ASVO takes it from the source job.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ImagingJobFlow2Params", module = "mwa_giant_squid")]
pub struct PyImagingJobFlow2Params {
    pub source_job_id: NonZeroU64,
    pub obs_id: Option<ObsId>,
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
    /// The ID of the conversion job to image. Not 0 (`ValueError` when it is set).
    #[getter]
    fn source_job_id(&self) -> JobId {
        JobId(self.source_job_id)
    }

    #[setter]
    fn set_source_job_id(&mut self, source_job_id: JobId) {
        self.source_job_id = source_job_id.0;
    }

    /// The Obs ID, or `None`, which leaves it out of the request (the MWA ASVO takes it from the source
    /// job). An `int` or an `ObsId` is accepted, and checked when it is set (`ObsIdError` if it is not valid).
    #[getter]
    fn obs_id(&self) -> Option<PyObsId> {
        self.obs_id.map(PyObsId)
    }

    #[setter]
    fn set_obs_id(&mut self, obs_id: Option<ObsIdArg>) {
        self.obs_id = obs_id.map(|o| o.0);
    }

    /// Check the values against the limits of the MWA ASVO, with the library's `validate_image_from_job_params`. The submit
    /// methods do this before they send a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        validate::validate_image_from_job_params(&self.to_schema(py)?).map_err(|e| api_error(py, e))
    }

    #[new]
    #[pyo3(
        text_signature = "(source_job_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', apply_primary_beam=True, auto_mask=3, auto_threshold=0.5, abs_threshold=0.001, channels_out=4, clean_iterations=100000, clean_threshold=0.001, image_size=3072, join_channels=True, join_polarizations=False, mgain=0.8, multiscale=False, nmiter=10, nwlayers=None, obs_id=None, output_mode='ImageOutputMode.Fits', pixel_scale=20.0, pol='Polarization.Xxyy', robust=-0.5, uvw_max=None, uvw_min=75.0, weighting='ImageWeighting.Briggs', wstack_nwlayers=None, allow_resubmit=False)"
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
        obs_id = None,
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
        source_job_id: JobId,
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
        obs_id: Option<ObsIdArg>,
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
            source_job_id: source_job_id.0,
            obs_id: obs_id.map(|o| o.0),
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
        let mut params = schema::image_from_job_params(self.source_job_id);
        params.obs_id = self.obs_id.map(i64::from);
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

impl PyDownloadJobParams {
    /// The class for the library's request body `params`, which is for `obs_id`.
    pub(super) fn from_schema(obs_id: ObsId, params: DownloadJobParams) -> Self {
        Self {
            obs_id,
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            allow_resubmit: params.allow_resubmit,
            only_get_metadata: params.only_get_metadata,
            offset: params.offset,
            duration: params.duration,
            from_channel: params.from_channel,
            to_channel: params.to_channel,
        }
    }
}

impl PyConversionJobParams {
    /// The class for the library's request body `params`, which is for `obs_id`.
    pub(super) fn from_schema(obs_id: ObsId, params: ConversionJobParams) -> Self {
        Self {
            obs_id,
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            output: params.output.into(),
            avg_freq_res: params.avg_freq_res,
            avg_time_res: params.avg_time_res,
            flag_edge_width: params.flag_edge_width,
            apply_di_cal: params.apply_di_cal,
            centre: params.centre.into(),
            custom_centre_ra: params.custom_centre_ra,
            custom_centre_dec: params.custom_centre_dec,
            no_apply_amps: params.no_apply_amps,
            no_digital_gains: params.no_digital_gains,
            no_flag_dc: params.no_flag_dc,
            no_geometric_delay: params.no_geometric_delay,
            no_passband_gains: params.no_passband_gains,
            no_cable_delay: params.no_cable_delay,
            no_rfi: params.no_rfi,
            allow_resubmit: params.allow_resubmit,
        }
    }
}

impl PyImagingJobFlow1Params {
    /// The class for the library's request body `params`, which is for `obs_id`.
    pub(super) fn from_schema(obs_id: ObsId, params: ImagingJobFlow1Params) -> Self {
        Self {
            obs_id,
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            apply_di_cal: params.apply_di_cal,
            apply_primary_beam: params.apply_primary_beam,
            auto_mask: params.auto_mask,
            auto_threshold: params.auto_threshold,
            abs_threshold: params.abs_threshold,
            avg_freq_res: params.avg_freq_res,
            avg_time_res: params.avg_time_res,
            channels_out: params.channels_out,
            clean_iterations: params.clean_iterations,
            clean_threshold: params.clean_threshold,
            centre: params.centre.into(),
            custom_centre_dec: params.custom_centre_dec,
            custom_centre_ra: params.custom_centre_ra,
            flag_edge_width: params.flag_edge_width,
            image_size: i64::from(params.image_size),
            join_channels: params.join_channels,
            join_polarizations: params.join_polarizations,
            mgain: params.mgain,
            multiscale: params.multiscale,
            nmiter: params.nmiter.get(),
            no_apply_amps: params.no_apply_amps,
            no_digital_gains: params.no_digital_gains,
            no_flag_dc: params.no_flag_dc,
            no_geometric_delay: params.no_geometric_delay,
            no_passband_gains: params.no_passband_gains,
            no_cable_delay: params.no_cable_delay,
            no_rfi: params.no_rfi,
            nwlayers: params.nwlayers,
            output_mode: params.output_mode.into(),
            pixel_scale: params.pixel_scale,
            pol: params.pol.into(),
            robust: params.robust,
            uvw_max: params.uvw_max,
            uvw_min: params.uvw_min,
            weighting: params.weighting.into(),
            wstack_nwlayers: params.wstack_nwlayers,
            allow_resubmit: params.allow_resubmit,
        }
    }
}

impl PyImagingJobFlow2Params {
    /// The class for the library's request body `params`.
    pub(super) fn from_schema(params: ImagingJobFlow2Params) -> Self {
        Self {
            source_job_id: params.source_job_id,
            obs_id: params
                .obs_id
                .and_then(|o| u64::try_from(o).ok())
                .and_then(|o| ObsId::validate(o).ok()),
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            apply_primary_beam: params.apply_primary_beam,
            auto_mask: params.auto_mask,
            auto_threshold: params.auto_threshold,
            abs_threshold: params.abs_threshold,
            channels_out: params.channels_out,
            clean_iterations: params.clean_iterations,
            clean_threshold: params.clean_threshold,
            image_size: i64::from(params.image_size),
            join_channels: params.join_channels,
            join_polarizations: params.join_polarizations,
            mgain: params.mgain,
            multiscale: params.multiscale,
            nmiter: params.nmiter.get(),
            nwlayers: params.nwlayers,
            output_mode: params.output_mode.into(),
            pixel_scale: params.pixel_scale,
            pol: params.pol.into(),
            robust: params.robust,
            uvw_max: params.uvw_max,
            uvw_min: params.uvw_min,
            weighting: params.weighting.into(),
            wstack_nwlayers: params.wstack_nwlayers,
            allow_resubmit: params.allow_resubmit,
        }
    }
}

/// The request body of a preprocessing job: the library's `PreprocessJobParams`. It is one of the
/// three classes that the preprocess endpoint takes. The MWA ASVO tells them apart by their fields,
/// so the body is sent as the class that it holds.
///
/// `PreprocessJobParams.Conversion(params)`, `PreprocessJobParams.Imaging(params)` and
/// `PreprocessJobParams.ImageFromJob(params)` make one from a `ConversionJobParams`, an
/// `ImagingJobFlow1Params` and an `ImagingJobFlow2Params`. `submit_preprocess_job` takes this class,
/// or one of the three classes (which it takes as the same variant).
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[pyclass(
    frozen,
    skip_from_py_object,
    name = "PreprocessJobParams",
    module = "mwa_giant_squid"
)]
pub enum PyPreprocessJobParams {
    /// A conversion job.
    Conversion {
        /// The request body.
        params: Py<PyConversionJobParams>,
    },
    /// An imaging job from an Obs ID (flow 1).
    Imaging {
        /// The request body.
        params: Py<PyImagingJobFlow1Params>,
    },
    /// An imaging job from a conversion job (flow 2).
    ImageFromJob {
        /// The request body.
        params: Py<PyImagingJobFlow2Params>,
    },
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyPreprocessJobParams {
    /// Check the values against the limits of the MWA ASVO, with the `validate_*_params` function of
    /// the class that it holds. `submit_preprocess_job` does this before it sends a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        self.to_schema(py)?.validate().map_err(|e| api_error(py, e))
    }
}

impl PyPreprocessJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ValueError` if the image size or `nmiter` is not valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<PreprocessJobParams> {
        match self {
            Self::Conversion { params } => Ok(params.borrow(py).to_schema().into()),
            Self::Imaging { params } => Ok(params.borrow(py).to_schema(py)?.into()),
            Self::ImageFromJob { params } => Ok(params.borrow(py).to_schema(py)?.into()),
        }
    }
}

/// The request body of a visibility, metadata, voltage or beamformer download job for `obs_id`, with
/// the MWA ASVO's defaults: the library's `download_params`. It is `DownloadJobParams(obs_id)`.
///
/// Args:
///     obs_id: The Obs ID.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ObsIdError: `obs_id` is not a valid Obs ID.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn download_params(obs_id: ObsIdArg) -> PyDownloadJobParams {
    PyDownloadJobParams::from_schema(obs_id.0, schema::download_params(obs_id.0))
}

/// The request body of a metadata download job for `obs_id`: a download job with `only_get_metadata`
/// set (the library's `metadata_params`).
///
/// Args:
///     obs_id: The Obs ID.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ObsIdError: `obs_id` is not a valid Obs ID.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn metadata_params(obs_id: ObsIdArg) -> PyDownloadJobParams {
    PyDownloadJobParams::from_schema(obs_id.0, schema::metadata_params(obs_id.0))
}

/// The request body of a voltage download job for `obs_id`, from `offset` seconds after the start of
/// the observation, for `duration` seconds (the library's `voltage_params`). The other values are the
/// defaults of every download job. The MWA ASVO refuses a voltage job whose delivery it does not allow
/// (it is delivered to Scratch as files).
///
/// Args:
///     obs_id: The Obs ID.
///     offset: The offset in seconds from the start GPS time of the observation, from 0 to 5400. It is
///         checked by `validate()` and when the job is submitted.
///     duration: The duration to download, in seconds.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ObsIdError: `obs_id` is not a valid Obs ID.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn voltage_params(obs_id: ObsIdArg, offset: i64, duration: u64) -> PyDownloadJobParams {
    PyDownloadJobParams::from_schema(obs_id.0, schema::voltage_params(obs_id.0, offset, duration))
}

/// The request body of a conversion job for `obs_id`, with the MWA ASVO's defaults: the library's
/// `conversion_params`. It is `ConversionJobParams(obs_id)`.
///
/// Args:
///     obs_id: The Obs ID.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ObsIdError: `obs_id` is not a valid Obs ID.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn conversion_params(obs_id: ObsIdArg) -> PyConversionJobParams {
    PyConversionJobParams::from_schema(obs_id.0, schema::conversion_params(obs_id.0))
}

/// The request body of an imaging job from an Obs ID (flow 1) for `obs_id`, with the MWA ASVO's
/// defaults: the library's `imaging_params`. It is `ImagingJobFlow1Params(obs_id)`.
///
/// Args:
///     obs_id: The Obs ID.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ObsIdError: `obs_id` is not a valid Obs ID.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn imaging_params(obs_id: ObsIdArg) -> PyImagingJobFlow1Params {
    PyImagingJobFlow1Params::from_schema(obs_id.0, schema::imaging_params(obs_id.0))
}

/// The request body of an imaging job (flow 2) for an image of the conversion job `source_job_id`,
/// with the MWA ASVO's defaults: the library's `image_from_job_params`. It is
/// `ImagingJobFlow2Params(source_job_id)`. Its `obs_id` is `None`: the MWA ASVO takes it from the
/// source job.
///
/// Args:
///     source_job_id: The ID of the conversion job to image.
///
/// Returns:
///     The request body.
///
/// Raises:
///     ValueError: `source_job_id` is 0.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn image_from_job_params(source_job_id: JobId) -> PyImagingJobFlow2Params {
    PyImagingJobFlow2Params::from_schema(schema::image_from_job_params(source_job_id.0))
}

/// The `params` argument of `submit_preprocess_job`: a `PreprocessJobParams`, or one of the three
/// classes that it holds, turned into the library's request body.
pub struct PreprocessParams(pub PreprocessJobParams);

impl<'a, 'py> FromPyObject<'a, 'py> for PreprocessParams {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        let py = obj.py();
        if let Ok(params) = obj.cast::<PyPreprocessJobParams>() {
            return Ok(Self(params.get().to_schema(py)?));
        }
        if let Ok(params) = obj.cast::<PyConversionJobParams>() {
            return Ok(Self(params.borrow().to_schema().into()));
        }
        if let Ok(params) = obj.cast::<PyImagingJobFlow1Params>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        if let Ok(params) = obj.cast::<PyImagingJobFlow2Params>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        Err(PyTypeError::new_err(format!(
            "expected a PreprocessJobParams, a ConversionJobParams, an ImagingJobFlow1Params or an \
             ImagingJobFlow2Params, not {}",
            obj.get_type().name()?
        )))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for PreprocessParams {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo {
            name: "PreprocessJobParams | ConversionJobParams | ImagingJobFlow1Params | ImagingJobFlow2Params".to_string(),
            source_module: None,
            import: Default::default(),
            type_refs: std::collections::HashMap::new(),
        }
    }
}
