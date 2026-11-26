// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The request bodies of the submit methods, as Python classes.
//!
//! Each class is the schema type of one job request body
//! (`DownloadJobParams`, `ConversionJobParams`, `ImagingJobParams`,
//! `ImageFromJobParams`),
//! with its name and field names. The arguments
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
//! The functions `download_params`, `conversion_params`, `imaging_params`
//! and `image_from_job_params` are the library's functions of the same
//! names: they make the class for the body that the library makes.
//! [`PyPreprocessJobParams`] is the library's `PreprocessJobParams`.
//!
//! `output_mode` of the imaging bodies is required (the schema gives it no
//! default), so it is a required keyword argument of the imaging classes
//! and an argument of `imaging_params` and `image_from_job_params`.

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
    ConversionJobParams, DownloadJobParams, ImageFromJobParams, ImagingJobParams,
};
use crate::mwa_asvo::api::validate;
use crate::obs_id::ObsId;

/// The request body of a download job: the schema's `DownloadJobParams`. It is the body of a
/// visibility, metadata, voltage or beamformer job: the MWA ASVO finds the kind of data from the
/// observation. Set `only_metadata` for a metadata job, and `offset`, `duration`, `from_channel`
/// and `to_channel` for a voltage job (`only_metadata=True` with any of them is an
/// `MwaAsvoApiErrorInvalidParameter`); the MWA ASVO refuses the voltage fields for an observation
/// that has no voltages.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// MWA ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
///
/// Args:
#[doc = concat!("    obs_id: ", cmd_help_text!(download_job_params, obs_id))]
#[doc = concat!("    delivery: ", cmd_help_text!(download_job_params, delivery))]
#[doc = concat!("    delivery_format: ", cmd_help_text!(download_job_params, delivery_format))]
#[doc = concat!("    allow_resubmit: ", cmd_help_text!(download_job_params, allow_resubmit))]
#[doc = concat!("    only_metadata: ", cmd_help_text!(download_job_params, only_metadata))]
#[doc = concat!("    offset: ", cmd_help_text!(download_job_params, offset))]
#[doc = concat!("    duration: ", cmd_help_text!(download_job_params, duration))]
#[doc = concat!("    from_channel: ", cmd_help_text!(download_job_params, from_channel))]
#[doc = concat!("    to_channel: ", cmd_help_text!(download_job_params, to_channel))]
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
    pub only_metadata: Option<bool>,
    #[pyo3(get, set)]
    pub offset: Option<i64>,
    #[pyo3(get, set)]
    pub duration: Option<i64>,
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

    /// Check the values against the limits of the MWA ASVO, with the library's
    /// `validate_download_params`. The submit methods do this before they send a request.
    ///
    /// Raises:
    ///     ValueError: A value is outside the limits (the first that is).
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        validate::validate_download_params(&self.to_schema()).map_err(|e| api_error(py, e))
    }

    #[new]
    #[pyo3(
        text_signature = "(obs_id, *, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', allow_resubmit=False, only_metadata=None, offset=None, duration=None, from_channel=None, to_channel=None)"
    )]
    #[pyo3(signature = (
        obs_id,
        *,
        delivery = PyDeliveryDestination::from(schema::download_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::download_defaults().delivery_format),
        allow_resubmit = schema::download_defaults().allow_resubmit,
        only_metadata = schema::download_defaults().only_metadata,
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
        only_metadata: Option<bool>,
        offset: Option<i64>,
        duration: Option<i64>,
        from_channel: Option<u8>,
        to_channel: Option<u8>,
    ) -> Self {
        Self {
            obs_id: obs_id.0,
            delivery,
            delivery_format,
            allow_resubmit,
            only_metadata,
            offset,
            duration,
            from_channel,
            to_channel,
        }
    }
}

impl PyDownloadJobParams {
    /// The library's request body for these values.
    pub(super) fn to_schema(&self) -> DownloadJobParams {
        DownloadJobParams {
            delivery: self.delivery.into(),
            delivery_format: self.delivery_format.into(),
            allow_resubmit: self.allow_resubmit,
            only_metadata: self.only_metadata,
            offset: self.offset,
            duration: self.duration,
            from_channel: self.from_channel,
            to_channel: self.to_channel,
            ..schema::download_params(self.obs_id)
        }
    }

    /// The class for the library's request body `params`, which is for `obs_id`.
    pub(super) fn from_schema(obs_id: ObsId, params: DownloadJobParams) -> Self {
        Self {
            obs_id,
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            allow_resubmit: params.allow_resubmit,
            only_metadata: params.only_metadata,
            offset: params.offset,
            duration: params.duration,
            from_channel: params.from_channel,
            to_channel: params.to_channel,
        }
    }
}

/// The request body of a conversion (preprocessing) job: the schema's `ConversionJobParams`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// MWA ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
///
/// Args:
#[doc = concat!("    obs_id: ", cmd_help_text!(conversion_job_params, obs_id))]
#[doc = concat!("    delivery: ", cmd_help_text!(conversion_job_params, delivery))]
#[doc = concat!("    delivery_format: ", cmd_help_text!(conversion_job_params, delivery_format))]
#[doc = concat!("    output: ", cmd_help_text!(conversion_job_params, output))]
#[doc = concat!("    avg_freq_res: ", cmd_help_text!(conversion_job_params, avg_freq_res))]
#[doc = concat!("    avg_time_res: ", cmd_help_text!(conversion_job_params, avg_time_res))]
#[doc = concat!("    flag_edge_width: ", cmd_help_text!(conversion_job_params, flag_edge_width))]
#[doc = concat!("    apply_di_cal: ", cmd_help_text!(conversion_job_params, apply_di_cal))]
#[doc = concat!("    centre: ", cmd_help_text!(conversion_job_params, centre))]
#[doc = concat!("    custom_centre_ra: ", cmd_help_text!(conversion_job_params, custom_centre_ra))]
#[doc = concat!("    custom_centre_dec: ", cmd_help_text!(conversion_job_params, custom_centre_dec))]
#[doc = concat!("    no_apply_amps: ", cmd_help_text!(conversion_job_params, no_apply_amps))]
#[doc = concat!("    no_digital_gains: ", cmd_help_text!(conversion_job_params, no_digital_gains))]
#[doc = concat!("    no_flag_dc: ", cmd_help_text!(conversion_job_params, no_flag_dc))]
#[doc = concat!("    no_geometric_delay: ", cmd_help_text!(conversion_job_params, no_geometric_delay))]
#[doc = concat!("    no_passband_gains: ", cmd_help_text!(conversion_job_params, no_passband_gains))]
#[doc = concat!("    no_cable_delay: ", cmd_help_text!(conversion_job_params, no_cable_delay))]
#[doc = concat!("    no_rfi: ", cmd_help_text!(conversion_job_params, no_rfi))]
#[doc = concat!("    allow_resubmit: ", cmd_help_text!(conversion_job_params, allow_resubmit))]
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
    pub flag_edge_width: Option<f64>,
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
        flag_edge_width: Option<f64>,
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

/// The request body of an imaging job that starts from an Obs ID: the schema's
/// `ImagingJobParams`.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// MWA ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// The Obs ID is checked when it is set (`ObsIdError`). The other values are checked by `validate()` and when the
/// job is submitted (see the submit method).
///
/// Args:
#[doc = concat!("    obs_id: ", cmd_help_text!(imaging_job_params, obs_id))]
#[doc = concat!("    output_mode: ", cmd_help_text!(imaging_job_params, output_mode))]
#[doc = concat!("    delivery: ", cmd_help_text!(imaging_job_params, delivery))]
#[doc = concat!("    delivery_format: ", cmd_help_text!(imaging_job_params, delivery_format))]
#[doc = concat!("    apply_di_cal: ", cmd_help_text!(imaging_job_params, apply_di_cal))]
#[doc = concat!("    apply_primary_beam: ", cmd_help_text!(imaging_job_params, apply_primary_beam))]
#[doc = concat!("    auto_mask: ", cmd_help_text!(imaging_job_params, auto_mask))]
#[doc = concat!("    auto_threshold: ", cmd_help_text!(imaging_job_params, auto_threshold))]
#[doc = concat!("    abs_threshold: ", cmd_help_text!(imaging_job_params, abs_threshold))]
#[doc = concat!("    avg_freq_res: ", cmd_help_text!(imaging_job_params, avg_freq_res))]
#[doc = concat!("    avg_time_res: ", cmd_help_text!(imaging_job_params, avg_time_res))]
#[doc = concat!("    channels_out: ", cmd_help_text!(imaging_job_params, channels_out))]
#[doc = concat!("    clean_iterations: ", cmd_help_text!(imaging_job_params, clean_iterations))]
#[doc = concat!("    centre: ", cmd_help_text!(imaging_job_params, centre))]
#[doc = concat!("    custom_centre_dec: ", cmd_help_text!(imaging_job_params, custom_centre_dec))]
#[doc = concat!("    custom_centre_ra: ", cmd_help_text!(imaging_job_params, custom_centre_ra))]
#[doc = concat!("    flag_edge_width: ", cmd_help_text!(imaging_job_params, flag_edge_width))]
#[doc = concat!("    image_size: ", cmd_help_text!(imaging_job_params, image_size))]
#[doc = concat!("    join_channels: ", cmd_help_text!(imaging_job_params, join_channels))]
#[doc = concat!("    join_polarizations: ", cmd_help_text!(imaging_job_params, join_polarizations))]
#[doc = concat!("    mgain: ", cmd_help_text!(imaging_job_params, mgain))]
#[doc = concat!("    multiscale: ", cmd_help_text!(imaging_job_params, multiscale))]
#[doc = concat!("    nmiter: ", cmd_help_text!(imaging_job_params, nmiter))]
#[doc = concat!("    no_apply_amps: ", cmd_help_text!(imaging_job_params, no_apply_amps))]
#[doc = concat!("    no_digital_gains: ", cmd_help_text!(imaging_job_params, no_digital_gains))]
#[doc = concat!("    no_flag_dc: ", cmd_help_text!(imaging_job_params, no_flag_dc))]
#[doc = concat!("    no_geometric_delay: ", cmd_help_text!(imaging_job_params, no_geometric_delay))]
#[doc = concat!("    no_passband_gains: ", cmd_help_text!(imaging_job_params, no_passband_gains))]
#[doc = concat!("    no_cable_delay: ", cmd_help_text!(imaging_job_params, no_cable_delay))]
#[doc = concat!("    no_rfi: ", cmd_help_text!(imaging_job_params, no_rfi))]
#[doc = concat!("    pixel_scale: ", cmd_help_text!(imaging_job_params, pixel_scale))]
#[doc = concat!("    polarization: ", cmd_help_text!(imaging_job_params, polarization))]
#[doc = concat!("    robust: ", cmd_help_text!(imaging_job_params, robust))]
#[doc = concat!("    uv_max: ", cmd_help_text!(imaging_job_params, uv_max))]
#[doc = concat!("    uv_min: ", cmd_help_text!(imaging_job_params, uv_min))]
#[doc = concat!("    weighting: ", cmd_help_text!(imaging_job_params, weighting))]
#[doc = concat!("    wstack_nwlayers: ", cmd_help_text!(imaging_job_params, wstack_nwlayers))]
#[doc = concat!("    allow_resubmit: ", cmd_help_text!(imaging_job_params, allow_resubmit))]
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ImagingJobParams", module = "mwa_giant_squid")]
pub struct PyImagingJobParams {
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
    pub channels_out: u64,
    #[pyo3(get, set)]
    pub clean_iterations: i64,
    #[pyo3(get, set)]
    pub centre: PyPhaseCentre,
    #[pyo3(get, set)]
    pub custom_centre_dec: Option<f64>,
    #[pyo3(get, set)]
    pub custom_centre_ra: Option<f64>,
    #[pyo3(get, set)]
    pub flag_edge_width: Option<f64>,
    #[pyo3(get, set)]
    pub image_size: Option<i64>,
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
    pub output_mode: PyImageOutputMode,
    #[pyo3(get, set)]
    pub pixel_scale: Option<f64>,
    #[pyo3(get, set)]
    pub polarization: PyPolarization,
    #[pyo3(get, set)]
    pub robust: f64,
    #[pyo3(get, set)]
    pub uv_max: Option<f64>,
    #[pyo3(get, set)]
    pub uv_min: f64,
    #[pyo3(get, set)]
    pub weighting: PyImageWeighting,
    #[pyo3(get, set)]
    pub wstack_nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyImagingJobParams {
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
        text_signature = "(obs_id, *, output_mode, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', apply_di_cal=True, apply_primary_beam=True, auto_mask=3, auto_threshold=0.5, abs_threshold=0.001, avg_freq_res=40.0, avg_time_res=2.0, channels_out=4, clean_iterations=100000, centre='PhaseCentre.Phase', custom_centre_dec=None, custom_centre_ra=None, flag_edge_width=80.0, image_size=None, join_channels=True, join_polarizations=False, mgain=0.8, multiscale=False, nmiter=10, no_apply_amps=False, no_digital_gains=False, no_flag_dc=False, no_geometric_delay=False, no_passband_gains=False, no_cable_delay=False, no_rfi=False, pixel_scale=None, polarization='Polarization.I', robust=-0.5, uv_max=None, uv_min=75.0, weighting='ImageWeighting.Briggs', wstack_nwlayers=None, allow_resubmit=False)"
    )]
    #[pyo3(signature = (
        obs_id,
        *,
        output_mode,
        delivery = PyDeliveryDestination::from(schema::imaging_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::imaging_defaults().delivery_format),
        apply_di_cal = schema::imaging_defaults().apply_di_cal,
        apply_primary_beam = schema::imaging_defaults().apply_primary_beam,
        auto_mask = schema::imaging_defaults().auto_mask,
        auto_threshold = schema::imaging_defaults().auto_threshold,
        abs_threshold = schema::imaging_defaults().abs_threshold,
        avg_freq_res = schema::imaging_defaults().avg_freq_res,
        avg_time_res = schema::imaging_defaults().avg_time_res,
        channels_out = schema::imaging_defaults().channels_out.get(),
        clean_iterations = schema::imaging_defaults().clean_iterations,
        centre = PyPhaseCentre::from(schema::imaging_defaults().centre),
        custom_centre_dec = schema::imaging_defaults().custom_centre_dec,
        custom_centre_ra = schema::imaging_defaults().custom_centre_ra,
        flag_edge_width = schema::imaging_defaults().flag_edge_width,
        image_size = schema::imaging_defaults().image_size.map(i64::from),
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
        pixel_scale = schema::imaging_defaults().pixel_scale,
        polarization = PyPolarization::from(schema::imaging_defaults().polarization),
        robust = schema::imaging_defaults().robust,
        uv_max = schema::imaging_defaults().uv_max,
        uv_min = schema::imaging_defaults().uv_min,
        weighting = PyImageWeighting::from(schema::imaging_defaults().weighting),
        wstack_nwlayers = schema::imaging_defaults().wstack_nwlayers,
        allow_resubmit = schema::imaging_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        obs_id: ObsIdArg,
        output_mode: PyImageOutputMode,
        delivery: PyDeliveryDestination,
        delivery_format: PyDeliveryFormat,
        apply_di_cal: bool,
        apply_primary_beam: bool,
        auto_mask: i64,
        auto_threshold: f64,
        abs_threshold: Option<f64>,
        avg_freq_res: Option<f64>,
        avg_time_res: Option<f64>,
        channels_out: u64,
        clean_iterations: i64,
        centre: PyPhaseCentre,
        custom_centre_dec: Option<f64>,
        custom_centre_ra: Option<f64>,
        flag_edge_width: Option<f64>,
        image_size: Option<i64>,
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
        pixel_scale: Option<f64>,
        polarization: PyPolarization,
        robust: f64,
        uv_max: Option<f64>,
        uv_min: f64,
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
            output_mode,
            pixel_scale,
            polarization,
            robust,
            uv_max,
            uv_min,
            weighting,
            wstack_nwlayers,
            allow_resubmit,
        }
    }
}

impl PyImagingJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size, `nmiter` or
    /// `channels_out` is not valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<ImagingJobParams> {
        let image_size = self
            .image_size
            .map(validate::image_size)
            .transpose()
            .map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(self.nmiter).map_err(|e| api_error(py, e))?;
        let channels_out =
            validate::channels_out(self.channels_out).map_err(|e| api_error(py, e))?;
        let mut params = schema::imaging_params(self.obs_id, self.output_mode.into());
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.apply_di_cal = self.apply_di_cal;
        params.apply_primary_beam = self.apply_primary_beam;
        params.auto_mask = self.auto_mask;
        params.auto_threshold = self.auto_threshold;
        params.abs_threshold = self.abs_threshold;
        params.avg_freq_res = self.avg_freq_res;
        params.avg_time_res = self.avg_time_res;
        params.channels_out = channels_out;
        params.clean_iterations = self.clean_iterations;
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
        params.pixel_scale = self.pixel_scale;
        params.polarization = self.polarization.into();
        params.robust = self.robust;
        params.uv_max = self.uv_max;
        params.uv_min = self.uv_min;
        params.weighting = self.weighting.into();
        params.wstack_nwlayers = self.wstack_nwlayers;
        params.allow_resubmit = self.allow_resubmit;
        Ok(params)
    }
}

/// The request body of an imaging job that starts from a conversion job: the
/// schema's `ImageFromJobParams`. It has no Obs ID: the MWA ASVO takes the Obs ID from the
/// source job.
///
/// Every keyword argument has the MWA ASVO's default as its default value
/// (the type stubs show it). `None` is accepted by the arguments that the MWA
/// MWA ASVO allows to be `null`, and is sent as `null`: it does not mean "use the
/// default". Each argument is also an attribute, which can be read and changed.
/// `source_job_id` must be greater than zero (checked when it is set, `ValueError`). The other
/// values are checked by `validate()` and when the job is submitted (see the submit method).
///
/// Args:
#[doc = concat!("    source_job_id: ", cmd_help_text!(image_from_job_params, source_job_id))]
#[doc = concat!("    output_mode: ", cmd_help_text!(image_from_job_params, output_mode))]
#[doc = concat!("    delivery: ", cmd_help_text!(image_from_job_params, delivery))]
#[doc = concat!("    delivery_format: ", cmd_help_text!(image_from_job_params, delivery_format))]
#[doc = concat!("    apply_primary_beam: ", cmd_help_text!(image_from_job_params, apply_primary_beam))]
#[doc = concat!("    auto_mask: ", cmd_help_text!(image_from_job_params, auto_mask))]
#[doc = concat!("    auto_threshold: ", cmd_help_text!(image_from_job_params, auto_threshold))]
#[doc = concat!("    abs_threshold: ", cmd_help_text!(image_from_job_params, abs_threshold))]
#[doc = concat!("    channels_out: ", cmd_help_text!(image_from_job_params, channels_out))]
#[doc = concat!("    clean_iterations: ", cmd_help_text!(image_from_job_params, clean_iterations))]
#[doc = concat!("    image_size: ", cmd_help_text!(image_from_job_params, image_size))]
#[doc = concat!("    join_channels: ", cmd_help_text!(image_from_job_params, join_channels))]
#[doc = concat!("    join_polarizations: ", cmd_help_text!(image_from_job_params, join_polarizations))]
#[doc = concat!("    mgain: ", cmd_help_text!(image_from_job_params, mgain))]
#[doc = concat!("    multiscale: ", cmd_help_text!(image_from_job_params, multiscale))]
#[doc = concat!("    nmiter: ", cmd_help_text!(image_from_job_params, nmiter))]
#[doc = concat!("    pixel_scale: ", cmd_help_text!(image_from_job_params, pixel_scale))]
#[doc = concat!("    polarization: ", cmd_help_text!(image_from_job_params, polarization))]
#[doc = concat!("    robust: ", cmd_help_text!(image_from_job_params, robust))]
#[doc = concat!("    uv_max: ", cmd_help_text!(image_from_job_params, uv_max))]
#[doc = concat!("    uv_min: ", cmd_help_text!(image_from_job_params, uv_min))]
#[doc = concat!("    weighting: ", cmd_help_text!(image_from_job_params, weighting))]
#[doc = concat!("    wstack_nwlayers: ", cmd_help_text!(image_from_job_params, wstack_nwlayers))]
#[doc = concat!("    allow_resubmit: ", cmd_help_text!(image_from_job_params, allow_resubmit))]
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "ImageFromJobParams", module = "mwa_giant_squid")]
pub struct PyImageFromJobParams {
    pub source_job_id: NonZeroU64,
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
    pub channels_out: u64,
    #[pyo3(get, set)]
    pub clean_iterations: i64,
    #[pyo3(get, set)]
    pub image_size: Option<i64>,
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
    pub output_mode: PyImageOutputMode,
    #[pyo3(get, set)]
    pub pixel_scale: Option<f64>,
    #[pyo3(get, set)]
    pub polarization: PyPolarization,
    #[pyo3(get, set)]
    pub robust: f64,
    #[pyo3(get, set)]
    pub uv_max: Option<f64>,
    #[pyo3(get, set)]
    pub uv_min: f64,
    #[pyo3(get, set)]
    pub weighting: PyImageWeighting,
    #[pyo3(get, set)]
    pub wstack_nwlayers: Option<i64>,
    #[pyo3(get, set)]
    pub allow_resubmit: Option<bool>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyImageFromJobParams {
    /// The ID of the conversion job to image. Not 0 (`ValueError` when it is set).
    #[getter]
    fn source_job_id(&self) -> JobId {
        JobId(self.source_job_id)
    }

    #[setter]
    fn set_source_job_id(&mut self, source_job_id: JobId) {
        self.source_job_id = source_job_id.0;
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
        text_signature = "(source_job_id, *, output_mode, delivery='DeliveryDestination.Acacia', delivery_format='DeliveryFormat.Tar', apply_primary_beam=True, auto_mask=3, auto_threshold=0.5, abs_threshold=0.001, channels_out=4, clean_iterations=100000, image_size=None, join_channels=True, join_polarizations=False, mgain=0.8, multiscale=False, nmiter=10, pixel_scale=None, polarization='Polarization.I', robust=-0.5, uv_max=None, uv_min=75.0, weighting='ImageWeighting.Briggs', wstack_nwlayers=None, allow_resubmit=False)"
    )]
    #[pyo3(signature = (
        source_job_id,
        *,
        output_mode,
        delivery = PyDeliveryDestination::from(schema::image_from_job_defaults().delivery),
        delivery_format = PyDeliveryFormat::from(schema::image_from_job_defaults().delivery_format),
        apply_primary_beam = schema::image_from_job_defaults().apply_primary_beam,
        auto_mask = schema::image_from_job_defaults().auto_mask,
        auto_threshold = schema::image_from_job_defaults().auto_threshold,
        abs_threshold = schema::image_from_job_defaults().abs_threshold,
        channels_out = schema::image_from_job_defaults().channels_out.get(),
        clean_iterations = schema::image_from_job_defaults().clean_iterations,
        image_size = schema::image_from_job_defaults().image_size.map(i64::from),
        join_channels = schema::image_from_job_defaults().join_channels,
        join_polarizations = schema::image_from_job_defaults().join_polarizations,
        mgain = schema::image_from_job_defaults().mgain,
        multiscale = schema::image_from_job_defaults().multiscale,
        nmiter = schema::image_from_job_defaults().nmiter.get(),
        pixel_scale = schema::image_from_job_defaults().pixel_scale,
        polarization = PyPolarization::from(schema::image_from_job_defaults().polarization),
        robust = schema::image_from_job_defaults().robust,
        uv_max = schema::image_from_job_defaults().uv_max,
        uv_min = schema::image_from_job_defaults().uv_min,
        weighting = PyImageWeighting::from(schema::image_from_job_defaults().weighting),
        wstack_nwlayers = schema::image_from_job_defaults().wstack_nwlayers,
        allow_resubmit = schema::image_from_job_defaults().allow_resubmit,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        source_job_id: JobId,
        output_mode: PyImageOutputMode,
        delivery: PyDeliveryDestination,
        delivery_format: PyDeliveryFormat,
        apply_primary_beam: bool,
        auto_mask: i64,
        auto_threshold: f64,
        abs_threshold: Option<f64>,
        channels_out: u64,
        clean_iterations: i64,
        image_size: Option<i64>,
        join_channels: bool,
        join_polarizations: bool,
        mgain: f64,
        multiscale: bool,
        nmiter: u64,
        pixel_scale: Option<f64>,
        polarization: PyPolarization,
        robust: f64,
        uv_max: Option<f64>,
        uv_min: f64,
        weighting: PyImageWeighting,
        wstack_nwlayers: Option<i64>,
        allow_resubmit: Option<bool>,
    ) -> Self {
        Self {
            source_job_id: source_job_id.0,
            delivery,
            delivery_format,
            apply_primary_beam,
            auto_mask,
            auto_threshold,
            abs_threshold,
            channels_out,
            clean_iterations,
            image_size,
            join_channels,
            join_polarizations,
            mgain,
            multiscale,
            nmiter,
            output_mode,
            pixel_scale,
            polarization,
            robust,
            uv_max,
            uv_min,
            weighting,
            wstack_nwlayers,
            allow_resubmit,
        }
    }
}

impl PyImageFromJobParams {
    /// The library's request body for these values.
    ///
    /// # Errors
    ///
    /// `ObsIdError` if the Obs ID is not valid. `ValueError` if the Job ID, image size, `nmiter` or
    /// `channels_out` is not valid.
    pub(super) fn to_schema(&self, py: Python<'_>) -> PyResult<ImageFromJobParams> {
        let image_size = self
            .image_size
            .map(validate::image_size)
            .transpose()
            .map_err(|e| api_error(py, e))?;
        let nmiter = validate::nmiter(self.nmiter).map_err(|e| api_error(py, e))?;
        let channels_out =
            validate::channels_out(self.channels_out).map_err(|e| api_error(py, e))?;
        let mut params = schema::image_from_job_params(self.source_job_id, self.output_mode.into());
        params.delivery = self.delivery.into();
        params.delivery_format = self.delivery_format.into();
        params.apply_primary_beam = self.apply_primary_beam;
        params.auto_mask = self.auto_mask;
        params.auto_threshold = self.auto_threshold;
        params.abs_threshold = self.abs_threshold;
        params.channels_out = channels_out;
        params.clean_iterations = self.clean_iterations;
        params.image_size = image_size;
        params.join_channels = self.join_channels;
        params.join_polarizations = self.join_polarizations;
        params.mgain = self.mgain;
        params.multiscale = self.multiscale;
        params.nmiter = nmiter;
        params.pixel_scale = self.pixel_scale;
        params.polarization = self.polarization.into();
        params.robust = self.robust;
        params.uv_max = self.uv_max;
        params.uv_min = self.uv_min;
        params.weighting = self.weighting.into();
        params.wstack_nwlayers = self.wstack_nwlayers;
        params.allow_resubmit = self.allow_resubmit;
        Ok(params)
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

impl PyImagingJobParams {
    /// The class for the library's request body `params`, which is for `obs_id`.
    pub(super) fn from_schema(obs_id: ObsId, params: ImagingJobParams) -> Self {
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
            channels_out: params.channels_out.get(),
            clean_iterations: params.clean_iterations,
            centre: params.centre.into(),
            custom_centre_dec: params.custom_centre_dec,
            custom_centre_ra: params.custom_centre_ra,
            flag_edge_width: params.flag_edge_width,
            image_size: params.image_size.map(i64::from),
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
            output_mode: params.output_mode.into(),
            pixel_scale: params.pixel_scale,
            polarization: params.polarization.into(),
            robust: params.robust,
            uv_max: params.uv_max,
            uv_min: params.uv_min,
            weighting: params.weighting.into(),
            wstack_nwlayers: params.wstack_nwlayers,
            allow_resubmit: params.allow_resubmit,
        }
    }
}

impl PyImageFromJobParams {
    /// The class for the library's request body `params`.
    pub(super) fn from_schema(params: ImageFromJobParams) -> Self {
        Self {
            source_job_id: params.source_job_id,
            delivery: params.delivery.into(),
            delivery_format: params.delivery_format.into(),
            apply_primary_beam: params.apply_primary_beam,
            auto_mask: params.auto_mask,
            auto_threshold: params.auto_threshold,
            abs_threshold: params.abs_threshold,
            channels_out: params.channels_out.get(),
            clean_iterations: params.clean_iterations,
            image_size: params.image_size.map(i64::from),
            join_channels: params.join_channels,
            join_polarizations: params.join_polarizations,
            mgain: params.mgain,
            multiscale: params.multiscale,
            nmiter: params.nmiter.get(),
            output_mode: params.output_mode.into(),
            pixel_scale: params.pixel_scale,
            polarization: params.polarization.into(),
            robust: params.robust,
            uv_max: params.uv_max,
            uv_min: params.uv_min,
            weighting: params.weighting.into(),
            wstack_nwlayers: params.wstack_nwlayers,
            allow_resubmit: params.allow_resubmit,
        }
    }
}

/// The request body of a preprocessing job: the library's `PreprocessJobParams`. It is one of the
/// two classes that the preprocess endpoint takes. The MWA ASVO tells them apart by their fields,
/// so the body is sent as the class that it holds. An imaging job from a conversion job
/// has its own endpoint: `submit_image_from_job`.
///
/// `PreprocessJobParams.Conversion(params)` and `PreprocessJobParams.Imaging(params)` make one from
/// a `ConversionJobParams` and an `ImagingJobParams`. `submit_preprocess_job` takes this class,
/// or one of the two classes (which it takes as the same variant).
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[pyclass(frozen, name = "PreprocessJobParams", module = "mwa_giant_squid")]
pub enum PyPreprocessJobParams {
    /// A conversion job.
    Conversion {
        /// The request body.
        params: Py<PyConversionJobParams>,
    },
    /// An imaging job from an Obs ID.
    Imaging {
        /// The request body.
        params: Py<PyImagingJobParams>,
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
        }
    }
}

/// The request body of a download job for `obs_id`, with the MWA ASVO's defaults: the library's
/// `download_params`. It is `DownloadJobParams(obs_id)`.
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

/// The request body of an imaging job from an Obs ID for `obs_id`, with the MWA ASVO's
/// defaults: the library's `imaging_params`. It is
/// `ImagingJobParams(obs_id, output_mode=output_mode)`.
///
/// Args:
///     obs_id: The Obs ID.
///     output_mode: The products to return (the schema gives it no default).
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
pub(super) fn imaging_params(
    obs_id: ObsIdArg,
    output_mode: PyImageOutputMode,
) -> PyImagingJobParams {
    PyImagingJobParams::from_schema(
        obs_id.0,
        schema::imaging_params(obs_id.0, output_mode.into()),
    )
}

/// The request body of an imaging job from a job: an image of the conversion job `source_job_id`,
/// with the MWA ASVO's defaults: the library's `image_from_job_params`. It is
/// `ImageFromJobParams(source_job_id, output_mode=output_mode)`. The MWA ASVO takes the Obs ID
/// from the source job.
///
/// Args:
///     source_job_id: The ID of the conversion job to image.
///     output_mode: The products to return (the schema gives it no default).
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
pub(super) fn image_from_job_params(
    source_job_id: JobId,
    output_mode: PyImageOutputMode,
) -> PyImageFromJobParams {
    PyImageFromJobParams::from_schema(schema::image_from_job_params(
        source_job_id.0,
        output_mode.into(),
    ))
}

/// The `params` argument of `submit_preprocess_job`: a `PreprocessJobParams`, or one of the two
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
        if let Ok(params) = obj.cast::<PyImagingJobParams>() {
            return Ok(Self(params.borrow().to_schema(py)?.into()));
        }
        Err(PyTypeError::new_err(format!(
            "expected a PreprocessJobParams, a ConversionJobParams or an ImagingJobParams, \
             not {}",
            obj.get_type().name()?
        )))
    }
}

#[cfg(feature = "python-stubgen")]
impl pyo3_stub_gen::PyStubType for PreprocessParams {
    fn type_output() -> pyo3_stub_gen::TypeInfo {
        pyo3_stub_gen::TypeInfo {
            name: "PreprocessJobParams | ConversionJobParams | ImagingJobParams".to_string(),
            source_module: None,
            import: Default::default(),
            type_refs: std::collections::HashMap::new(),
        }
    }
}
