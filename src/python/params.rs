// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Request bodies for the submit methods, built from Python arguments.
//!
//! Every optional Python argument is `None` by default. A `None` argument is
//! left unset on the request builder, so the value comes from the OpenAPI
//! schema default. This layer adds no defaults of its own, the same rule as
//! the CLI's.
//!
//! There is one argument struct per job type. The submit methods build it
//! from their keyword arguments; step 2.3's `*_params` functions will do the
//! same.
//!
//! The Python argument names are the OpenAPI field names. The CLI-only
//! options `mode` (beamformer) and the voltage `delivery_format` are not
//! arguments, because the CLI does not expose them either.

use std::num::NonZeroU64;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::types::{
    PyCentre, PyDelivery, PyDeliveryFormat, PyOutput, PyOutputMode, PyPolarization, PyWeighting,
};
use crate::asvo::apiv2::openapi::{
    self as api, BeamformerJobParams, ConversionJobParams, DownloadJobParams,
    ImagingJobFlow1Params, ImagingJobFlow2Params, VoltageJobParams,
};
use crate::obsid::Obsid;

/// Validate an obsid and give it the type the request bodies use.
///
/// # Errors
///
/// `ValueError` if `obs_id` is not a valid obsid.
pub(super) fn obs_id_to_i64(obs_id: u64) -> PyResult<i64> {
    let obsid = Obsid::validate(obs_id).map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(i64::try_from(u64::from(obsid)).expect("Obsid's validated range always fits in i64"))
}

/// A non-zero job ID from a Python integer.
fn non_zero(name: &str, value: u64) -> PyResult<NonZeroU64> {
    NonZeroU64::new(value)
        .ok_or_else(|| PyValueError::new_err(format!("{name} must be greater than zero")))
}

/// An error from a request builder, as a `ValueError`.
fn builder_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Set each field of `$builder` whose Python argument is not `None`.
macro_rules! set_if_some {
    ($builder:ident, $($field:ident => $value:expr),+ $(,)?) => {
        $(
            if let Some(value) = $value {
                $builder = $builder.$field(value);
            }
        )+
    };
}

/// The arguments of a visibility or metadata download job.
pub(super) struct DownloadArgs {
    pub delivery: Option<PyDelivery>,
    pub delivery_format: Option<PyDeliveryFormat>,
    pub allow_resubmit: Option<bool>,
}

impl DownloadArgs {
    /// The request body. The library sets `download_type`.
    pub(super) fn into_params(self, obs_id: u64) -> PyResult<DownloadJobParams> {
        let mut builder = DownloadJobParams::builder().obs_id(obs_id_to_i64(obs_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery.map(api::Delivery::from),
            delivery_format => self.delivery_format.map(api::DeliveryFormat::from),
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}

/// The arguments of a conversion job.
pub(super) struct ConversionArgs {
    pub delivery: Option<PyDelivery>,
    pub delivery_format: Option<PyDeliveryFormat>,
    pub output: Option<PyOutput>,
    pub avg_freq_res: Option<f64>,
    pub avg_time_res: Option<f64>,
    pub flag_edge_width: Option<f64>,
    pub apply_di_cal: Option<bool>,
    pub centre: Option<PyCentre>,
    pub custom_centre_ra: Option<f64>,
    pub custom_centre_dec: Option<f64>,
    pub no_apply_amps: Option<bool>,
    pub no_digital_gains: Option<bool>,
    pub no_flag_dc: Option<bool>,
    pub no_geometry_delay: Option<bool>,
    pub no_passband_gains: Option<bool>,
    pub allow_resubmit: Option<bool>,
}

impl ConversionArgs {
    /// The request body.
    pub(super) fn into_params(self, obs_id: u64) -> PyResult<ConversionJobParams> {
        let mut builder = ConversionJobParams::builder().obs_id(obs_id_to_i64(obs_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery.map(api::Delivery::from),
            delivery_format => self.delivery_format.map(api::DeliveryFormat::from),
            output => self.output.map(api::Output::from),
            avg_freq_res => self.avg_freq_res,
            avg_time_res => self.avg_time_res,
            flag_edge_width => self.flag_edge_width,
            apply_di_cal => self.apply_di_cal,
            centre => self.centre.map(api::Centre::from),
            custom_centre_ra => self.custom_centre_ra,
            custom_centre_dec => self.custom_centre_dec,
            no_apply_amps => self.no_apply_amps,
            no_digital_gains => self.no_digital_gains,
            no_flag_dc => self.no_flag_dc,
            no_geometry_delay => self.no_geometry_delay,
            no_passband_gains => self.no_passband_gains,
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}

/// The image size, checked against the sizes the API accepts.
fn image_size(value: i64) -> PyResult<api::ImageSizes> {
    api::ImageSizes::try_from(value).map_err(|_| {
        PyValueError::new_err(format!(
            "image_size={value} is not one of the sizes the MWA ASVO accepts"
        ))
    })
}

/// The `nmiter` value, which the API requires to be greater than zero.
fn nmiter(value: u64) -> PyResult<NonZeroU64> {
    non_zero("nmiter", value)
}

/// The arguments of an imaging job that starts from an obsid (flow 1).
pub(super) struct ImagingArgs {
    pub delivery: Option<PyDelivery>,
    pub delivery_format: Option<PyDeliveryFormat>,
    pub apply_di_cal: Option<bool>,
    pub apply_primary_beam: Option<bool>,
    pub auto_mask: Option<i64>,
    pub auto_threshold: Option<f64>,
    pub abs_threshold: Option<f64>,
    pub avg_freq_res: Option<f64>,
    pub avg_time_res: Option<f64>,
    pub channels_out: Option<i64>,
    pub clean_iterations: Option<i64>,
    pub clean_threshold: Option<f64>,
    pub centre: Option<PyCentre>,
    pub custom_centre_dec: Option<f64>,
    pub custom_centre_ra: Option<f64>,
    pub flag_edge_width: Option<f64>,
    pub image_size: Option<i64>,
    pub join_channels: Option<bool>,
    pub join_polarizations: Option<bool>,
    pub mgain: Option<f64>,
    pub multiscale: Option<bool>,
    pub nmiter: Option<u64>,
    pub no_apply_amps: Option<bool>,
    pub nwlayers: Option<i64>,
    pub output_mode: Option<PyOutputMode>,
    pub pixel_scale: Option<f64>,
    pub pol: Option<PyPolarization>,
    pub robust: Option<f64>,
    pub uvw_max: Option<f64>,
    pub uvw_min: Option<f64>,
    pub weighting: Option<PyWeighting>,
    pub wstack_nwlayers: Option<i64>,
    pub allow_resubmit: Option<bool>,
}

impl ImagingArgs {
    /// The request body.
    pub(super) fn into_params(self, obs_id: u64) -> PyResult<ImagingJobFlow1Params> {
        let mut builder = ImagingJobFlow1Params::builder().obs_id(obs_id_to_i64(obs_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery.map(api::Delivery::from),
            delivery_format => self.delivery_format.map(api::DeliveryFormat::from),
            apply_di_cal => self.apply_di_cal,
            apply_primary_beam => self.apply_primary_beam,
            auto_mask => self.auto_mask,
            auto_threshold => self.auto_threshold,
            abs_threshold => self.abs_threshold,
            avg_freq_res => self.avg_freq_res,
            avg_time_res => self.avg_time_res,
            channels_out => self.channels_out,
            clean_iterations => self.clean_iterations,
            clean_threshold => self.clean_threshold,
            centre => self.centre.map(api::Centre::from),
            custom_centre_dec => self.custom_centre_dec,
            custom_centre_ra => self.custom_centre_ra,
            flag_edge_width => self.flag_edge_width,
            image_size => self.image_size.map(image_size).transpose()?,
            join_channels => self.join_channels,
            join_polarizations => self.join_polarizations,
            mgain => self.mgain,
            multiscale => self.multiscale,
            nmiter => self.nmiter.map(nmiter).transpose()?,
            no_apply_amps => self.no_apply_amps,
            nwlayers => self.nwlayers,
            output_mode => self.output_mode.map(api::OutputMode::from),
            pixel_scale => self.pixel_scale,
            pol => self.pol.map(api::Polarization::from),
            robust => self.robust,
            uvw_max => self.uvw_max,
            uvw_min => self.uvw_min,
            weighting => self.weighting.map(api::Weighting::from),
            wstack_nwlayers => self.wstack_nwlayers,
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}

/// The arguments of an imaging job that starts from an existing conversion
/// job (flow 2).
pub(super) struct ImageFromJobArgs {
    pub delivery: Option<PyDelivery>,
    pub delivery_format: Option<PyDeliveryFormat>,
    pub apply_primary_beam: Option<bool>,
    pub auto_mask: Option<i64>,
    pub auto_threshold: Option<f64>,
    pub abs_threshold: Option<f64>,
    pub channels_out: Option<i64>,
    pub clean_iterations: Option<i64>,
    pub clean_threshold: Option<f64>,
    pub image_size: Option<i64>,
    pub join_channels: Option<bool>,
    pub join_polarizations: Option<bool>,
    pub mgain: Option<f64>,
    pub multiscale: Option<bool>,
    pub nmiter: Option<u64>,
    pub nwlayers: Option<i64>,
    pub output_mode: Option<PyOutputMode>,
    pub pixel_scale: Option<f64>,
    /// Free-form, as in the schema: this endpoint does not use the
    /// `Polarization` enum.
    pub pol: Option<String>,
    pub robust: Option<f64>,
    pub uvw_max: Option<f64>,
    pub uvw_min: Option<f64>,
    pub weighting: Option<PyWeighting>,
    pub wstack_nwlayers: Option<i64>,
    pub allow_resubmit: Option<bool>,
}

impl ImageFromJobArgs {
    /// The request body.
    pub(super) fn into_params(
        self,
        obs_id: u64,
        source_job_id: u64,
    ) -> PyResult<ImagingJobFlow2Params> {
        let mut builder = ImagingJobFlow2Params::builder()
            .obs_id(obs_id_to_i64(obs_id)?)
            .source_job_id(non_zero("source_job_id", source_job_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery.map(api::Delivery::from),
            delivery_format => self.delivery_format.map(api::DeliveryFormat::from),
            apply_primary_beam => self.apply_primary_beam,
            auto_mask => self.auto_mask,
            auto_threshold => self.auto_threshold,
            abs_threshold => self.abs_threshold,
            channels_out => self.channels_out,
            clean_iterations => self.clean_iterations,
            clean_threshold => self.clean_threshold,
            image_size => self.image_size.map(image_size).transpose()?,
            join_channels => self.join_channels,
            join_polarizations => self.join_polarizations,
            mgain => self.mgain,
            multiscale => self.multiscale,
            nmiter => self.nmiter.map(nmiter).transpose()?,
            nwlayers => self.nwlayers,
            output_mode => self.output_mode.map(api::OutputMode::from),
            pixel_scale => self.pixel_scale,
            pol => self.pol,
            robust => self.robust,
            uvw_max => self.uvw_max,
            uvw_min => self.uvw_min,
            weighting => self.weighting.map(api::Weighting::from),
            wstack_nwlayers => self.wstack_nwlayers,
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}

/// The arguments of a voltage download job.
pub(super) struct VoltageArgs {
    pub offset: i64,
    pub duration: u64,
    /// A string, as in the schema. The only valid value is "scratch".
    pub delivery: Option<String>,
    pub from_channel: Option<u8>,
    pub to_channel: Option<u8>,
    pub allow_resubmit: Option<bool>,
}

impl VoltageArgs {
    /// The request body. As in the CLI, `channel_range` is derived: it is
    /// set when either channel bound is given.
    pub(super) fn into_params(self, obs_id: u64) -> PyResult<VoltageJobParams> {
        let channel_range = self.from_channel.is_some() || self.to_channel.is_some();

        let mut builder = VoltageJobParams::builder()
            .obs_id(obs_id_to_i64(obs_id)?)
            .offset(self.offset)
            .duration(self.duration)
            .channel_range(channel_range);
        set_if_some!(
            builder,
            delivery => self.delivery,
            from_channel => self.from_channel,
            to_channel => self.to_channel,
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}

/// The arguments of a beamformer download job.
pub(super) struct BeamformerArgs {
    pub delivery: Option<PyDelivery>,
    pub delivery_format: Option<PyDeliveryFormat>,
    pub allow_resubmit: Option<bool>,
}

impl BeamformerArgs {
    /// The request body.
    pub(super) fn into_params(self, obs_id: u64) -> PyResult<BeamformerJobParams> {
        let mut builder = BeamformerJobParams::builder().obs_id(obs_id_to_i64(obs_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery.map(api::Delivery::from),
            delivery_format => self.delivery_format.map(api::DeliveryFormat::from),
            allow_resubmit => self.allow_resubmit,
        );
        builder.try_into().map_err(builder_error)
    }
}
