// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The arguments of each job type, and the request body made from them.
//!
//! There is one struct per job type. Its fields are the request fields of
//! the OpenAPI schema that a user can set, with the schema's names, and each
//! is an `Option`: `None` leaves the field unset, so the body has the
//! schema's default. The library adds no defaults of its own.
//!
//! The CLI (from its clap arguments) and the Python module (from its keyword
//! arguments) fill these structs, so the request bodies are made in one
//! place. `into_params` also makes the values that need a schema type of
//! their own (`image_size`, `nmiter`, `source_job_id`) and the derived
//! `channel_range` of a voltage job. The limits of the numbers are checked
//! by the client's submit methods ([`super::validate`]).

use super::openapi::{
    BeamformerJobParams, Centre, ConversionJobParams, Delivery, DeliveryFormat, DownloadJobParams,
    DownloadType, ImagingJobFlow1Params, ImagingJobFlow2Params, Output, OutputMode, Polarization,
    VoltageJobParams, Weighting,
};
use super::{validate, AsvoApiError};
use crate::obs_id::ObsId;

/// Set each field of `$builder` whose argument is not `None`.
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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DownloadArgs {
    pub delivery: Option<Delivery>,
    pub delivery_format: Option<DeliveryFormat>,
    pub allow_resubmit: Option<bool>,
}

impl DownloadArgs {
    /// The request body for `obs_id`, a download of `download_type`.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::Conversion`] if the body cannot be made.
    pub fn into_params(
        self,
        obs_id: ObsId,
        download_type: DownloadType,
    ) -> Result<DownloadJobParams, AsvoApiError> {
        let mut builder = DownloadJobParams::builder()
            .obs_id(i64::from(obs_id))
            .download_type(download_type);
        set_if_some!(
            builder,
            delivery => self.delivery,
            delivery_format => self.delivery_format,
            allow_resubmit => self.allow_resubmit,
        );
        Ok(builder.try_into()?)
    }
}

/// The arguments of a conversion job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConversionArgs {
    pub delivery: Option<Delivery>,
    pub delivery_format: Option<DeliveryFormat>,
    pub output: Option<Output>,
    pub avg_freq_res: Option<f64>,
    pub avg_time_res: Option<f64>,
    pub flag_edge_width: Option<f64>,
    pub apply_di_cal: Option<bool>,
    pub centre: Option<Centre>,
    pub custom_centre_ra: Option<f64>,
    pub custom_centre_dec: Option<f64>,
    pub no_apply_amps: Option<bool>,
    pub no_digital_gains: Option<bool>,
    pub no_flag_dc: Option<bool>,
    pub no_geometry_delay: Option<bool>,
    pub no_passband_gains: Option<bool>,
    pub no_cable_delay: Option<bool>,
    pub no_rfi: Option<bool>,
    pub allow_resubmit: Option<bool>,
}

impl ConversionArgs {
    /// The request body for `obs_id`.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::Conversion`] if the body cannot be made.
    pub fn into_params(self, obs_id: ObsId) -> Result<ConversionJobParams, AsvoApiError> {
        let mut builder = ConversionJobParams::builder().obs_id(i64::from(obs_id));
        set_if_some!(
            builder,
            delivery => self.delivery,
            delivery_format => self.delivery_format,
            output => self.output,
            avg_freq_res => self.avg_freq_res,
            avg_time_res => self.avg_time_res,
            flag_edge_width => self.flag_edge_width,
            apply_di_cal => self.apply_di_cal,
            centre => self.centre,
            custom_centre_ra => self.custom_centre_ra.map(Some),
            custom_centre_dec => self.custom_centre_dec.map(Some),
            no_apply_amps => self.no_apply_amps,
            no_digital_gains => self.no_digital_gains.map(Some),
            no_flag_dc => self.no_flag_dc.map(Some),
            no_geometry_delay => self.no_geometry_delay.map(Some),
            no_passband_gains => self.no_passband_gains.map(Some),
            no_cable_delay => self.no_cable_delay.map(Some),
            no_rfi => self.no_rfi.map(Some),
            allow_resubmit => self.allow_resubmit.map(Some),
        );
        Ok(builder.try_into()?)
    }
}

/// The arguments of an imaging job that starts from an obsid (flow 1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImagingArgs {
    pub delivery: Option<Delivery>,
    pub delivery_format: Option<DeliveryFormat>,
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
    pub centre: Option<Centre>,
    pub custom_centre_dec: Option<f64>,
    pub custom_centre_ra: Option<f64>,
    pub flag_edge_width: Option<f64>,
    /// One of the schema's image sizes ([`validate::image_size`]).
    pub image_size: Option<i64>,
    pub join_channels: Option<bool>,
    pub join_polarizations: Option<bool>,
    pub mgain: Option<f64>,
    pub multiscale: Option<bool>,
    /// At least 1 ([`validate::nmiter`]).
    pub nmiter: Option<u64>,
    pub no_apply_amps: Option<bool>,
    pub no_digital_gains: Option<bool>,
    pub no_flag_dc: Option<bool>,
    pub no_geometry_delay: Option<bool>,
    pub no_passband_gains: Option<bool>,
    pub no_cable_delay: Option<bool>,
    pub no_rfi: Option<bool>,
    pub nwlayers: Option<i64>,
    pub output_mode: Option<OutputMode>,
    pub pixel_scale: Option<f64>,
    pub pol: Option<Polarization>,
    pub robust: Option<f64>,
    pub uvw_max: Option<f64>,
    pub uvw_min: Option<f64>,
    pub weighting: Option<Weighting>,
    pub wstack_nwlayers: Option<i64>,
    pub allow_resubmit: Option<bool>,
}

impl ImagingArgs {
    /// The request body for `obs_id`.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] for an `image_size` or `nmiter`
    /// that the schema does not allow; [`AsvoApiError::Conversion`] if the
    /// body cannot be made.
    pub fn into_params(self, obs_id: ObsId) -> Result<ImagingJobFlow1Params, AsvoApiError> {
        let mut builder = ImagingJobFlow1Params::builder().obs_id(i64::from(obs_id));
        set_if_some!(
            builder,
            delivery => self.delivery,
            delivery_format => self.delivery_format,
            apply_di_cal => self.apply_di_cal,
            apply_primary_beam => self.apply_primary_beam,
            auto_mask => self.auto_mask,
            auto_threshold => self.auto_threshold,
            abs_threshold => self.abs_threshold.map(Some),
            avg_freq_res => self.avg_freq_res,
            avg_time_res => self.avg_time_res,
            channels_out => self.channels_out,
            clean_iterations => self.clean_iterations,
            clean_threshold => self.clean_threshold.map(Some),
            centre => self.centre,
            custom_centre_dec => self.custom_centre_dec.map(Some),
            custom_centre_ra => self.custom_centre_ra.map(Some),
            flag_edge_width => self.flag_edge_width,
            image_size => self.image_size.map(validate::image_size).transpose()?,
            join_channels => self.join_channels,
            join_polarizations => self.join_polarizations,
            mgain => self.mgain,
            multiscale => self.multiscale,
            nmiter => self.nmiter.map(validate::nmiter).transpose()?,
            no_apply_amps => self.no_apply_amps,
            no_digital_gains => self.no_digital_gains.map(Some),
            no_flag_dc => self.no_flag_dc.map(Some),
            no_geometry_delay => self.no_geometry_delay.map(Some),
            no_passband_gains => self.no_passband_gains.map(Some),
            no_cable_delay => self.no_cable_delay.map(Some),
            no_rfi => self.no_rfi.map(Some),
            nwlayers => self.nwlayers.map(Some),
            output_mode => self.output_mode,
            pixel_scale => self.pixel_scale,
            pol => self.pol,
            robust => self.robust,
            uvw_max => self.uvw_max.map(Some),
            uvw_min => self.uvw_min,
            weighting => self.weighting,
            wstack_nwlayers => self.wstack_nwlayers.map(Some),
            allow_resubmit => self.allow_resubmit.map(Some),
        );
        Ok(builder.try_into()?)
    }
}

/// The arguments of an imaging job that starts from an existing conversion
/// job (flow 2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageFromJobArgs {
    pub delivery: Option<Delivery>,
    pub delivery_format: Option<DeliveryFormat>,
    pub apply_primary_beam: Option<bool>,
    pub auto_mask: Option<i64>,
    pub auto_threshold: Option<f64>,
    pub abs_threshold: Option<f64>,
    pub channels_out: Option<i64>,
    pub clean_iterations: Option<i64>,
    pub clean_threshold: Option<f64>,
    /// One of the schema's image sizes ([`validate::image_size`]).
    pub image_size: Option<i64>,
    pub join_channels: Option<bool>,
    pub join_polarizations: Option<bool>,
    pub mgain: Option<f64>,
    pub multiscale: Option<bool>,
    /// At least 1 ([`validate::nmiter`]).
    pub nmiter: Option<u64>,
    pub nwlayers: Option<i64>,
    pub output_mode: Option<OutputMode>,
    pub pixel_scale: Option<f64>,
    pub pol: Option<Polarization>,
    pub robust: Option<f64>,
    pub uvw_max: Option<f64>,
    pub uvw_min: Option<f64>,
    pub weighting: Option<Weighting>,
    pub wstack_nwlayers: Option<i64>,
    pub allow_resubmit: Option<bool>,
}

impl ImageFromJobArgs {
    /// The request body for an image of the conversion job `source_job_id`
    /// of `obs_id`.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] for a `source_job_id` of 0, or an
    /// `image_size` or `nmiter` that the schema does not allow;
    /// [`AsvoApiError::Conversion`] if the body cannot be made.
    pub fn into_params(
        self,
        obs_id: ObsId,
        source_job_id: u64,
    ) -> Result<ImagingJobFlow2Params, AsvoApiError> {
        let mut builder = ImagingJobFlow2Params::builder()
            .obs_id(i64::from(obs_id))
            .source_job_id(validate::source_job_id(source_job_id)?);
        set_if_some!(
            builder,
            delivery => self.delivery,
            delivery_format => self.delivery_format,
            apply_primary_beam => self.apply_primary_beam,
            auto_mask => self.auto_mask,
            auto_threshold => self.auto_threshold,
            abs_threshold => self.abs_threshold.map(Some),
            channels_out => self.channels_out,
            clean_iterations => self.clean_iterations,
            clean_threshold => self.clean_threshold.map(Some),
            image_size => self.image_size.map(validate::image_size).transpose()?,
            join_channels => self.join_channels,
            join_polarizations => self.join_polarizations,
            mgain => self.mgain,
            multiscale => self.multiscale,
            nmiter => self.nmiter.map(validate::nmiter).transpose()?,
            nwlayers => self.nwlayers.map(Some),
            output_mode => self.output_mode,
            pixel_scale => self.pixel_scale,
            pol => self.pol,
            robust => self.robust,
            uvw_max => self.uvw_max.map(Some),
            uvw_min => self.uvw_min,
            weighting => self.weighting,
            wstack_nwlayers => self.wstack_nwlayers.map(Some),
            allow_resubmit => self.allow_resubmit.map(Some),
        );
        Ok(builder.try_into()?)
    }
}

/// The arguments of a voltage download job. `offset` and `duration` are
/// required by the schema.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoltageArgs {
    pub offset: i64,
    pub duration: u64,
    /// A string, as in the schema. The only valid value is "scratch".
    pub delivery: Option<String>,
    pub from_channel: Option<u8>,
    pub to_channel: Option<u8>,
    pub allow_resubmit: Option<bool>,
}

impl VoltageArgs {
    /// The request body for `obs_id`. `channel_range` is derived: it is set
    /// when either channel bound is given (the API needs it until it
    /// removes the field).
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::Conversion`] if the body cannot be made.
    pub fn into_params(self, obs_id: ObsId) -> Result<VoltageJobParams, AsvoApiError> {
        let channel_range = self.from_channel.is_some() || self.to_channel.is_some();
        let mut builder = VoltageJobParams::builder()
            .obs_id(i64::from(obs_id))
            .offset(self.offset)
            .duration(self.duration)
            .channel_range(Some(channel_range));
        set_if_some!(
            builder,
            delivery => self.delivery,
            from_channel => self.from_channel.map(Some),
            to_channel => self.to_channel.map(Some),
            allow_resubmit => self.allow_resubmit.map(Some),
        );
        Ok(builder.try_into()?)
    }
}

/// The arguments of a beamformer download job.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeamformerArgs {
    pub delivery: Option<Delivery>,
    pub delivery_format: Option<DeliveryFormat>,
    pub allow_resubmit: Option<bool>,
}

impl BeamformerArgs {
    /// The request body for `obs_id`.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::Conversion`] if the body cannot be made.
    pub fn into_params(self, obs_id: ObsId) -> Result<BeamformerJobParams, AsvoApiError> {
        let mut builder = BeamformerJobParams::builder().obs_id(i64::from(obs_id));
        set_if_some!(
            builder,
            delivery => self.delivery,
            delivery_format => self.delivery_format,
            allow_resubmit => self.allow_resubmit.map(Some),
        );
        Ok(builder.try_into()?)
    }
}

#[cfg(test)]
mod tests;
