// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Per-job-type CLI argument groups and their conversion into MWA ASVO
//! request bodies.
//!
//! Each struct here is a clap argument group (flattened into the relevant
//! [`super::Args`] variant) paired with a `to_params` method that builds the
//! generated OpenAPI request type. Keeping the argument-to-request-body
//! mapping in pure functions means it can be tested without a server: see
//! `src/cli/tests.rs`.

use std::num::NonZeroU64;

use clap::ArgAction;

use crate::mwa_asvo::api::job_args::{
    BeamformerArgs, ConversionArgs, DownloadArgs, ImageFromJobArgs, ImagingArgs, VoltageArgs,
};
use crate::mwa_asvo::api::openapi::{
    BeamformerJobParams, Centre, ConversionJobParams, Delivery, DeliveryFormat, DownloadJobParams,
    DownloadType, ImagingJobFlow1Params, ImagingJobFlow2Params, JobsByUserRequest, Output,
    OutputMode, Polarization, VoltageJobParams, Weighting,
};
use crate::mwa_asvo::api::validate::{self, Bounds};
use crate::mwa_asvo::{AsvoApiError, ENV_GIANT_SQUID_DELIVERY, ENV_GIANT_SQUID_DELIVERY_FORMAT};

use super::value_enums::SchemaEnumParser;
use crate::obs_id::ObsId;

/// The default of `list --days`: the schema's default for the `days` of a job
/// listing (`JobsByUserRequest`), so that the help shows it and the CLI cannot
/// drift from the API.
pub fn list_days_default() -> NonZeroU64 {
    JobsByUserRequest::default()
        .days
        .expect("BUG: the schema has no default for JobsByUserRequest.days")
}

/// Builds a clap value parser that only accepts an f64 within `bounds`.
/// The bounds are the library's ([`crate::mwa_asvo::api::validate`]), which
/// come from the MWA ASVO schema, so a value that is out of range is
/// rejected by the CLI at once rather than by the server.
pub fn parse_f64_bounds(bounds: Bounds) -> impl Fn(&str) -> Result<f64, String> + Clone {
    move |s: &str| {
        let v: f64 = s.parse().map_err(|e| format!("not a valid number: {e}"))?;
        if bounds.contains(v) {
            Ok(v)
        } else {
            Err(bounds.describe(v))
        }
    }
}

/// As [parse_f64_bounds], but for i64 fields.
pub fn parse_i64_bounds(bounds: Bounds) -> impl Fn(&str) -> Result<i64, String> + Clone {
    move |s: &str| {
        let v: i64 = s.parse().map_err(|e| format!("not a valid integer: {e}"))?;
        if bounds.contains(v as f64) {
            Ok(v)
        } else {
            Err(bounds.describe(v as f64))
        }
    }
}

/// The clap value parser of `list --days`: an integer from 1 to 30
/// ([`validate::DAYS`]), as the schema's type.
pub fn parse_days(s: &str) -> Result<NonZeroU64, String> {
    let days = parse_i64_bounds(validate::DAYS)(s)?;
    validate::days(days).map_err(|e| e.to_string())
}

/// The clap value parser of `list --date-from` and `--date-to`: the
/// library's [`crate::parse_utc_time`], with the error as text.
pub fn parse_utc_time(s: &str) -> Result<jiff::Timestamp, String> {
    crate::parse_utc_time(s).map_err(|e| e.to_string())
}

/// Validates a WSClean image size against the MWA ASVO API's fixed set of
/// allowed sizes (see [`validate::image_size`]).
pub fn parse_image_size(s: &str) -> Result<i64, String> {
    let v: i64 = s.parse().map_err(|e| format!("not a valid integer: {e}"))?;
    validate::image_size(v)
        .map(i64::from)
        .map_err(|e| e.to_string())
}

/// The parser of `--pol`: one of the API's `Polarization` values, kept as the
/// text that the request body carries. Both imaging endpoints take the enum
/// (flow 2 since schema v1.11), so an unsupported value is rejected by the
/// CLI rather than only when the request body is built.
fn polarization_parser() -> impl clap::builder::TypedValueParser<Value = String> {
    clap::builder::TypedValueParser::map(SchemaEnumParser::<Polarization>::new(), |p| p.to_string())
}

/// A [`ConversionJobParams`] populated entirely from the OpenAPI schema
/// defaults (via the generated builder's `Default`), used to source the
/// clap arg defaults below so they can't drift from the schema. `obs_id` is
/// required by the builder but irrelevant to the defaults - every real
/// submission sets its own - so we pass a placeholder.
pub fn conversion_defaults() -> ConversionJobParams {
    ConversionJobParams::builder()
        .obs_id(0_i64)
        .try_into()
        .expect("BUG: a required ConversionJobParams field is missing from conversion_defaults()")
}

/// A [`DownloadJobParams`] populated from the OpenAPI schema defaults, used
/// to source the SubmitVis/SubmitMeta clap arg defaults. As with
/// `conversion_defaults`, `obs_id` is a required placeholder.
pub fn download_defaults() -> DownloadJobParams {
    DownloadJobParams::builder()
        .obs_id(0_i64)
        .try_into()
        .expect("BUG: a required DownloadJobParams field is missing from download_defaults()")
}

/// A [`VoltageJobParams`] populated from the OpenAPI schema defaults, used
/// to source the SubmitVolt clap arg defaults. `obs_id`, `offset` and
/// `duration` are required placeholders - every real submission sets them.
pub fn voltage_defaults() -> VoltageJobParams {
    VoltageJobParams::builder()
        .obs_id(0_i64)
        .offset(0_i64)
        .duration(0_u64)
        .try_into()
        .expect("BUG: a required VoltageJobParams field is missing from voltage_defaults()")
}

/// A [`BeamformerJobParams`] populated from the OpenAPI schema defaults,
/// used to source the SubmitBf clap arg defaults. `obs_id` is a required
/// placeholder.
pub fn beamformer_defaults() -> BeamformerJobParams {
    BeamformerJobParams::builder()
        .obs_id(0_i64)
        .try_into()
        .expect("BUG: a required BeamformerJobParams field is missing from beamformer_defaults()")
}

/// An [`ImagingJobFlow1Params`] populated from the OpenAPI schema defaults,
/// used to source the SubmitImage clap arg defaults. `obs_id` is a required
/// placeholder.
pub fn imaging1_defaults() -> ImagingJobFlow1Params {
    ImagingJobFlow1Params::builder()
        .obs_id(0_i64)
        .try_into()
        .expect("BUG: a required ImagingJobFlow1Params field is missing from imaging1_defaults()")
}

/// An [`ImagingJobFlow2Params`] populated from the OpenAPI schema defaults,
/// used to source the SubmitImageFromJob clap arg defaults. `obs_id` and
/// `source_job_id` are required placeholders.
pub fn imaging2_defaults() -> ImagingJobFlow2Params {
    ImagingJobFlow2Params::builder()
        .obs_id(0_i64)
        .source_job_id(NonZeroU64::new(1).unwrap())
        .try_into()
        .expect("BUG: a required ImagingJobFlow2Params field is missing from imaging2_defaults()")
}

/// Arguments shared by the visibility and metadata download jobs, which use
/// the same request body and the same endpoint, differing only in their
/// `download_type`.
#[derive(clap::Args, Debug, Clone)]
pub struct DownloadJobArgs {
    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value_t = download_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<Delivery>::new())]
    pub delivery: Delivery,

    /// Tell MWA ASVO to deliver the data in a particular format.
    #[arg(short = 'f', long, default_value_t = download_defaults().delivery_format, env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl DownloadJobArgs {
    /// Build the request body for a raw visibility download job.
    pub fn to_vis_params(&self, obs_id: ObsId) -> Result<DownloadJobParams, AsvoApiError> {
        self.args().into_params(obs_id, DownloadType::Vis)
    }

    /// Build the request body for a metadata download job.
    pub fn to_meta_params(&self, obs_id: ObsId) -> Result<DownloadJobParams, AsvoApiError> {
        self.args().into_params(obs_id, DownloadType::Meta)
    }

    /// The library's arguments. The CLI has a value for every option (its
    /// defaults are the schema's), so every field is set.
    fn args(&self) -> DownloadArgs {
        DownloadArgs {
            delivery: Some(self.delivery),
            delivery_format: Some(self.delivery_format),
            allow_resubmit: Some(self.allow_resubmit),
        }
    }
}

/// Arguments for a preprocessing/conversion job.
#[derive(clap::Args, Debug, Clone)]
pub struct ConversionJobArgs {
    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value_t = conversion_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<Delivery>::new())]
    pub delivery: Delivery,

    /// Tell MWA ASVO to deliver the data in a particular format.
    #[arg(short = 'f', long, default_value_t = conversion_defaults().delivery_format, env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    /// Output format: "ms" (measurement set) or "uvfits".
    #[arg(short = 'o', long, default_value_t = conversion_defaults().output, value_parser = SchemaEnumParser::<Output>::new())]
    pub output: Output,

    /// Frequency resolution to average to (kHz).
    #[arg(long, default_value_t = conversion_defaults().avg_freq_res, value_parser = parse_f64_bounds(validate::AVG_FREQ_RES))]
    pub avg_freq_res: f64,

    /// Time resolution to average to (s).
    #[arg(long, default_value_t = conversion_defaults().avg_time_res, value_parser = parse_f64_bounds(validate::AVG_TIME_RES))]
    pub avg_time_res: f64,

    /// Width of frequency edge flagging (kHz).
    #[arg(long, default_value_t = conversion_defaults().flag_edge_width, value_parser = parse_f64_bounds(validate::FLAG_EDGE_WIDTH))]
    pub flag_edge_width: f64,

    /// Whether to apply the DI calibration solution.
    #[arg(long)]
    pub apply_di_cal: bool,

    /// Phase centre mode: "phase", "pointing", or "custom".
    /// If "custom", also supply --custom-centre-ra and --custom-centre-dec.
    #[arg(long, default_value_t = conversion_defaults().centre, value_parser = SchemaEnumParser::<Centre>::new())]
    pub centre: Centre,

    /// Custom phase centre right ascension (degrees). Requires --centre custom.
    #[arg(long, alias = "phase-centre-ra", value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_RA))]
    pub custom_centre_ra: Option<f64>,

    /// Custom phase centre declination (degrees). Requires --centre custom.
    #[arg(long, alias = "phase-centre-dec", value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_DEC))]
    pub custom_centre_dec: Option<f64>,

    /// Whether to skip applying amplitude calibration solutions.
    #[arg(long)]
    pub no_apply_amps: bool,

    /// Whether to skip applying digital gains.
    #[arg(long)]
    pub no_digital_gains: bool,

    /// Whether to skip flagging the DC channel.
    #[arg(long)]
    pub no_flag_dc: bool,

    /// Whether to skip applying geometric delay corrections.
    #[arg(long)]
    pub no_geometry_delay: bool,

    /// Whether to skip applying passband gain corrections.
    #[arg(long)]
    pub no_passband_gains: bool,

    /// Whether to skip applying cable delay corrections.
    #[arg(long)]
    pub no_cable_delay: bool,

    /// Whether to skip RFI flagging.
    #[arg(long)]
    pub no_rfi: bool,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl ConversionJobArgs {
    /// Build the request body for a conversion job for a single obsid, with
    /// the library's [`ConversionArgs`]. Every field is set (the CLI's
    /// defaults are the schema's).
    pub fn to_params(&self, obs_id: ObsId) -> Result<ConversionJobParams, AsvoApiError> {
        ConversionArgs {
            delivery: Some(self.delivery),
            delivery_format: Some(self.delivery_format),
            output: Some(self.output),
            avg_freq_res: Some(self.avg_freq_res),
            avg_time_res: Some(self.avg_time_res),
            flag_edge_width: Some(self.flag_edge_width),
            apply_di_cal: Some(self.apply_di_cal),
            centre: Some(self.centre),
            custom_centre_ra: self.custom_centre_ra,
            custom_centre_dec: self.custom_centre_dec,
            no_apply_amps: Some(self.no_apply_amps),
            no_digital_gains: Some(self.no_digital_gains),
            no_flag_dc: Some(self.no_flag_dc),
            no_geometry_delay: Some(self.no_geometry_delay),
            no_passband_gains: Some(self.no_passband_gains),
            no_cable_delay: Some(self.no_cable_delay),
            no_rfi: Some(self.no_rfi),
            allow_resubmit: Some(self.allow_resubmit),
        }
        .into_params(obs_id)
    }
}

/// Arguments for an imaging job that starts from raw visibilities (flow 1).
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingJobArgs {
    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value_t = imaging1_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<Delivery>::new())]
    pub delivery: Delivery,

    /// Tell MWA ASVO to deliver the data in a particular format.
    #[arg(short = 'f', long, default_value_t = imaging1_defaults().delivery_format, env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    /// Whether to apply the DI calibration solution.
    #[arg(
        long,
        default_value_t = imaging1_defaults().apply_di_cal,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_di_cal: bool,

    /// Whether to apply the primary beam correction.
    #[arg(
        long,
        default_value_t = imaging1_defaults().apply_primary_beam,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_primary_beam: bool,

    /// WSClean -auto-mask value.
    #[arg(long, default_value_t = imaging1_defaults().auto_mask, value_parser = parse_i64_bounds(validate::AUTO_MASK))]
    pub auto_mask: i64,

    /// WSClean -auto-threshold value.
    #[arg(long, default_value_t = imaging1_defaults().auto_threshold, value_parser = parse_f64_bounds(validate::AUTO_THRESHOLD))]
    pub auto_threshold: f64,

    /// Absolute cleaning threshold (Jy). Overridden by auto_threshold
    /// unless explicitly set.
    #[arg(long, default_value_t = imaging1_defaults().abs_threshold.unwrap(), value_parser = parse_f64_bounds(validate::ABS_THRESHOLD))]
    pub abs_threshold: f64,

    /// Frequency resolution to average to before imaging (kHz).
    #[arg(long, default_value_t = imaging1_defaults().avg_freq_res, value_parser = parse_f64_bounds(validate::AVG_FREQ_RES))]
    pub avg_freq_res: f64,

    /// Time resolution to average to before imaging (s).
    #[arg(long, default_value_t = imaging1_defaults().avg_time_res, value_parser = parse_f64_bounds(validate::AVG_TIME_RES))]
    pub avg_time_res: f64,

    /// Number of output channel groups.
    #[arg(long, default_value_t = imaging1_defaults().channels_out)]
    pub channels_out: i64,

    /// WSClean -niter value (max clean iterations).
    #[arg(long, default_value_t = imaging1_defaults().clean_iterations, value_parser = parse_i64_bounds(validate::CLEAN_ITERATIONS))]
    pub clean_iterations: i64,

    /// WSClean cleaning threshold (Jy). Takes precedence over
    /// auto_threshold if set.
    #[arg(long, default_value_t = imaging1_defaults().clean_threshold.unwrap(), value_parser = parse_f64_bounds(validate::CLEAN_THRESHOLD))]
    pub clean_threshold: f64,

    /// Custom phase centre declination (degrees). Requires --centre custom.
    #[arg(long, alias = "custom-dec", value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_DEC))]
    pub custom_centre_dec: Option<f64>,

    /// Custom phase centre right ascension (degrees). Requires --centre custom.
    #[arg(long, alias = "custom-ra", value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_RA))]
    pub custom_centre_ra: Option<f64>,

    /// Width of frequency edge flagging (kHz).
    #[arg(long, default_value_t = imaging1_defaults().flag_edge_width, value_parser = parse_f64_bounds(validate::FLAG_EDGE_WIDTH))]
    pub flag_edge_width: f64,

    /// WSClean image size in pixels.
    #[arg(long, default_value_t = *imaging1_defaults().image_size, value_parser = parse_image_size)]
    pub image_size: i64,

    /// Join output channel groups for cleaning.
    #[arg(
        long,
        default_value_t = imaging1_defaults().join_channels,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_channels: bool,

    /// Join polarisations for cleaning.
    #[arg(long)]
    pub join_polarizations: bool,

    /// WSClean -mgain value.
    #[arg(long, default_value_t = imaging1_defaults().mgain, value_parser = parse_f64_bounds(validate::MGAIN))]
    pub mgain: f64,

    /// Enable WSClean multiscale cleaning.
    #[arg(long)]
    pub multiscale: bool,

    /// WSClean -nmiter value (max major cleaning iterations).
    #[arg(long, default_value_t = imaging1_defaults().nmiter.get() as i64, value_parser = parse_i64_bounds(validate::NMITER))]
    pub nmiter: i64,

    /// Number of w-projection layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::NWLAYERS))]
    pub nwlayers: Option<i64>,

    /// The output mode / product to request.
    #[arg(short = 'o', long, default_value_t = imaging1_defaults().output_mode, value_parser = SchemaEnumParser::<OutputMode>::new())]
    pub output_mode: OutputMode,

    /// Where to centre the image: "phase", "pointing", or "custom".
    /// If "custom", also supply --custom-centre-ra and --custom-centre-dec.
    #[arg(long, alias = "phase-center", default_value_t = imaging1_defaults().centre, value_parser = SchemaEnumParser::<Centre>::new())]
    pub centre: Centre,

    /// Pixel scale (arcsec/pixel).
    #[arg(long, default_value_t = imaging1_defaults().pixel_scale, value_parser = parse_f64_bounds(validate::PIXEL_SCALE))]
    pub pixel_scale: f64,

    /// Polarisation to image: XX, YY or XXYY.
    #[arg(long, default_value_t = imaging1_defaults().pol.to_string(), value_parser = polarization_parser())]
    pub pol: String,

    /// WSClean -robust (Briggs robustness) value.
    #[arg(long, default_value_t = imaging1_defaults().robust, value_parser = parse_f64_bounds(validate::ROBUST))]
    pub robust: f64,

    /// Maximum uv distance to image, in wavelengths (upper bound on
    /// the range that can be requested).
    #[arg(long, value_parser = parse_f64_bounds(validate::UVW_MAX))]
    pub uvw_max: Option<f64>,

    /// Minimum uv distance to image, in wavelengths.
    #[arg(long, default_value_t = imaging1_defaults().uvw_min, value_parser = parse_f64_bounds(validate::UVW_MIN))]
    pub uvw_min: f64,

    /// WSClean weighting scheme.
    #[arg(long, default_value_t = imaging1_defaults().weighting, value_parser = SchemaEnumParser::<Weighting>::new())]
    pub weighting: Weighting,

    /// Number of w-stacking layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::WSTACK_NWLAYERS))]
    pub wstack_nwlayers: Option<i64>,

    /// Whether to skip applying amplitude calibration solutions.
    /// Leave at the default (false) unless you know you need this.
    #[arg(long)]
    pub no_apply_amps: bool,

    /// Whether to skip applying digital gains.
    #[arg(long)]
    pub no_digital_gains: bool,

    /// Whether to skip flagging the DC channel.
    #[arg(long)]
    pub no_flag_dc: bool,

    /// Whether to skip applying geometric delay corrections.
    #[arg(long)]
    pub no_geometry_delay: bool,

    /// Whether to skip applying passband gain corrections.
    #[arg(long)]
    pub no_passband_gains: bool,

    /// Whether to skip applying cable delay corrections.
    #[arg(long)]
    pub no_cable_delay: bool,

    /// Whether to skip RFI flagging.
    #[arg(long)]
    pub no_rfi: bool,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl ImagingJobArgs {
    /// Build the request body for an imaging job for a single obsid, with
    /// the library's [`ImagingArgs`]. Every field that has a value is set.
    pub fn to_params(&self, obs_id: ObsId) -> Result<ImagingJobFlow1Params, AsvoApiError> {
        ImagingArgs {
            delivery: Some(self.delivery),
            delivery_format: Some(self.delivery_format),
            apply_primary_beam: Some(self.apply_primary_beam),
            auto_mask: Some(self.auto_mask),
            auto_threshold: Some(self.auto_threshold),
            abs_threshold: Some(self.abs_threshold),
            channels_out: Some(self.channels_out),
            clean_iterations: Some(self.clean_iterations),
            clean_threshold: Some(self.clean_threshold),
            image_size: Some(self.image_size),
            join_channels: Some(self.join_channels),
            join_polarizations: Some(self.join_polarizations),
            mgain: Some(self.mgain),
            multiscale: Some(self.multiscale),
            // clap's range check keeps it at least 1; a value that is not a
            // `u64` becomes 0, which the library refuses with its message.
            nmiter: Some(u64::try_from(self.nmiter).unwrap_or_default()),
            nwlayers: self.nwlayers,
            output_mode: Some(self.output_mode),
            pixel_scale: Some(self.pixel_scale),
            pol: Some(self.pol.parse::<Polarization>()?),
            robust: Some(self.robust),
            uvw_max: self.uvw_max,
            uvw_min: Some(self.uvw_min),
            weighting: Some(self.weighting),
            wstack_nwlayers: self.wstack_nwlayers,
            allow_resubmit: Some(self.allow_resubmit),
            apply_di_cal: Some(self.apply_di_cal),
            avg_freq_res: Some(self.avg_freq_res),
            avg_time_res: Some(self.avg_time_res),
            centre: Some(self.centre),
            custom_centre_dec: self.custom_centre_dec,
            custom_centre_ra: self.custom_centre_ra,
            flag_edge_width: Some(self.flag_edge_width),
            no_apply_amps: Some(self.no_apply_amps),
            no_digital_gains: Some(self.no_digital_gains),
            no_flag_dc: Some(self.no_flag_dc),
            no_geometry_delay: Some(self.no_geometry_delay),
            no_passband_gains: Some(self.no_passband_gains),
            no_cable_delay: Some(self.no_cable_delay),
            no_rfi: Some(self.no_rfi),
        }
        .into_params(obs_id)
    }
}

/// Arguments for an imaging job that starts from an existing conversion job
/// (flow 2).
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingFromJobArgs {
    /// The MWA ASVO conversion job ID to image from. Required.
    #[arg(long)]
    pub source_job_id: NonZeroU64,

    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value_t = imaging2_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<Delivery>::new())]
    pub delivery: Delivery,

    /// Tell MWA ASVO to deliver the data in a particular format.
    #[arg(short = 'f', long, default_value_t = imaging2_defaults().delivery_format, env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    /// Whether to apply the primary beam correction.
    #[arg(
        long,
        default_value_t = imaging2_defaults().apply_primary_beam,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_primary_beam: bool,

    /// WSClean -auto-mask value.
    #[arg(long, default_value_t = imaging2_defaults().auto_mask, value_parser = parse_i64_bounds(validate::AUTO_MASK))]
    pub auto_mask: i64,

    /// WSClean -auto-threshold value.
    #[arg(long, default_value_t = imaging2_defaults().auto_threshold, value_parser = parse_f64_bounds(validate::AUTO_THRESHOLD))]
    pub auto_threshold: f64,

    /// Absolute cleaning threshold (Jy). Overridden by auto_threshold
    /// unless explicitly set.
    #[arg(long, default_value_t = imaging2_defaults().abs_threshold.unwrap(), value_parser = parse_f64_bounds(validate::ABS_THRESHOLD))]
    pub abs_threshold: f64,

    /// Number of output channel groups.
    #[arg(long, default_value_t = imaging2_defaults().channels_out)]
    pub channels_out: i64,

    /// WSClean -niter value (max clean iterations).
    #[arg(long, default_value_t = imaging2_defaults().clean_iterations, value_parser = parse_i64_bounds(validate::CLEAN_ITERATIONS))]
    pub clean_iterations: i64,

    /// WSClean cleaning threshold (Jy). Takes precedence over
    /// auto_threshold if set.
    #[arg(long, default_value_t = imaging2_defaults().clean_threshold.unwrap(), value_parser = parse_f64_bounds(validate::CLEAN_THRESHOLD))]
    pub clean_threshold: f64,

    /// WSClean image size in pixels.
    #[arg(long, default_value_t = *imaging2_defaults().image_size, value_parser = parse_image_size)]
    pub image_size: i64,

    /// Join output channel groups for cleaning.
    #[arg(
        long,
        default_value_t = imaging2_defaults().join_channels,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_channels: bool,

    /// Join polarisations for cleaning.
    #[arg(long)]
    pub join_polarizations: bool,

    /// WSClean -mgain value.
    #[arg(long, default_value_t = imaging2_defaults().mgain, value_parser = parse_f64_bounds(validate::MGAIN))]
    pub mgain: f64,

    /// Enable WSClean multiscale cleaning.
    #[arg(long)]
    pub multiscale: bool,

    /// WSClean -nmiter value (max major cleaning iterations).
    #[arg(long, default_value_t = imaging2_defaults().nmiter.get() as i64, value_parser = parse_i64_bounds(validate::NMITER))]
    pub nmiter: i64,

    /// Number of w-projection layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::NWLAYERS))]
    pub nwlayers: Option<i64>,

    /// The output mode / product to request.
    #[arg(short = 'o', long, default_value_t = imaging2_defaults().output_mode, value_parser = SchemaEnumParser::<OutputMode>::new())]
    pub output_mode: OutputMode,

    /// Pixel scale (arcsec/pixel).
    #[arg(long, default_value_t = imaging2_defaults().pixel_scale, value_parser = parse_f64_bounds(validate::PIXEL_SCALE))]
    pub pixel_scale: f64,

    /// Polarisation to image: XX, YY or XXYY.
    #[arg(long, default_value_t = imaging2_defaults().pol.to_string(), value_parser = polarization_parser())]
    pub pol: String,

    /// WSClean -robust (Briggs robustness) value.
    #[arg(long, default_value_t = imaging2_defaults().robust, value_parser = parse_f64_bounds(validate::ROBUST))]
    pub robust: f64,

    /// Maximum uv distance to image, in wavelengths (upper bound on
    /// the range that can be requested).
    #[arg(long, value_parser = parse_f64_bounds(validate::UVW_MAX))]
    pub uvw_max: Option<f64>,

    /// Minimum uv distance to image, in wavelengths.
    #[arg(long, default_value_t = imaging2_defaults().uvw_min, value_parser = parse_f64_bounds(validate::UVW_MIN))]
    pub uvw_min: f64,

    /// WSClean weighting scheme.
    #[arg(long, default_value_t = imaging2_defaults().weighting, value_parser = SchemaEnumParser::<Weighting>::new())]
    pub weighting: Weighting,

    /// Number of w-stacking layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::WSTACK_NWLAYERS))]
    pub wstack_nwlayers: Option<i64>,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl ImagingFromJobArgs {
    /// Build the request body for an image of the conversion job
    /// `--source-job-id`, with the library's [`ImageFromJobArgs`].
    pub fn to_params(&self, obs_id: ObsId) -> Result<ImagingJobFlow2Params, AsvoApiError> {
        ImageFromJobArgs {
            delivery: Some(self.delivery),
            delivery_format: Some(self.delivery_format),
            apply_primary_beam: Some(self.apply_primary_beam),
            auto_mask: Some(self.auto_mask),
            auto_threshold: Some(self.auto_threshold),
            abs_threshold: Some(self.abs_threshold),
            channels_out: Some(self.channels_out),
            clean_iterations: Some(self.clean_iterations),
            clean_threshold: Some(self.clean_threshold),
            image_size: Some(self.image_size),
            join_channels: Some(self.join_channels),
            join_polarizations: Some(self.join_polarizations),
            mgain: Some(self.mgain),
            multiscale: Some(self.multiscale),
            // clap's range check keeps it at least 1; a value that is not a
            // `u64` becomes 0, which the library refuses with its message.
            nmiter: Some(u64::try_from(self.nmiter).unwrap_or_default()),
            nwlayers: self.nwlayers,
            output_mode: Some(self.output_mode),
            pixel_scale: Some(self.pixel_scale),
            pol: Some(self.pol.parse::<Polarization>()?),
            robust: Some(self.robust),
            uvw_max: self.uvw_max,
            uvw_min: Some(self.uvw_min),
            weighting: Some(self.weighting),
            wstack_nwlayers: self.wstack_nwlayers,
            allow_resubmit: Some(self.allow_resubmit),
        }
        .into_params(obs_id, self.source_job_id.get())
    }
}

/// Arguments for a voltage download job.
#[derive(clap::Args, Debug, Clone)]
pub struct VoltageJobArgs {
    /// Tell MWA ASVO where to deliver the data. The only valid value for
    /// a voltage job is "scratch", which requires the "mwavcs" Pawsey
    /// Group on your MWA ASVO profile.
    #[arg(short, long, default_value_t = voltage_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY)]
    pub delivery: String,

    /// The offset in seconds from the start GPS time of the observation.
    #[arg(short, long, value_parser = parse_i64_bounds(validate::VOLTAGE_OFFSET))]
    pub offset: i64,

    /// The duration (in seconds) to download.
    #[arg(short = 'u', long)]
    pub duration: u64,

    /// The 'from' receiver channel number (0-255).
    #[arg(short = 'f', long)]
    pub from_channel: Option<u8>,

    /// The 'to' receiver channel number (0-255).
    #[arg(short = 't', long)]
    pub to_channel: Option<u8>,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl VoltageJobArgs {
    /// Build the request body for a voltage job, with the library's
    /// [`VoltageArgs`] (which derives `channel_range`).
    pub fn to_params(&self, obs_id: ObsId) -> Result<VoltageJobParams, AsvoApiError> {
        VoltageArgs {
            offset: self.offset,
            duration: self.duration,
            delivery: Some(self.delivery.clone()),
            from_channel: self.from_channel,
            to_channel: self.to_channel,
            allow_resubmit: Some(self.allow_resubmit),
        }
        .into_params(obs_id)
    }
}

/// Arguments for a beamformer download job.
#[derive(clap::Args, Debug, Clone)]
pub struct BeamformerJobArgs {
    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value_t = beamformer_defaults().delivery, env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<Delivery>::new())]
    pub delivery: Delivery,

    /// Tell MWA ASVO to deliver the data in a particular format.
    #[arg(short = 'f', long, default_value_t = beamformer_defaults().delivery_format, env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    /// Allow resubmitting a job even if an identical one has completed.
    #[arg(short = 'r', long, action = ArgAction::SetTrue)]
    pub allow_resubmit: bool,
}

impl BeamformerJobArgs {
    /// Build the request body for a beamformer job, with the library's
    /// [`BeamformerArgs`].
    pub fn to_params(&self, obs_id: ObsId) -> Result<BeamformerJobParams, AsvoApiError> {
        BeamformerArgs {
            delivery: Some(self.delivery),
            delivery_format: Some(self.delivery_format),
            allow_resubmit: Some(self.allow_resubmit),
        }
        .into_params(obs_id)
    }
}
