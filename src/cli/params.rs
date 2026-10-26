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

use std::marker::PhantomData;
use std::num::NonZeroU64;

use clap::ArgAction;

use crate::mwa_asvo::api::job_params::{
    conversion_defaults, conversion_params, download_defaults, download_params,
    image_from_job_defaults, image_from_job_params, imaging_defaults, imaging_params,
    voltage_defaults, voltage_params,
};
use crate::mwa_asvo::api::openapi::{
    ConversionJobParams, DeliveryDestination, DeliveryFormat, DownloadJobParams, DownloadType,
    ImageOutputMode, ImageSizes, ImageWeighting, ImagingJobFlow1Params, ImagingJobFlow2Params,
    JobsByUserRequest, OutputFormat, PhaseCentre, Polarization, VoltageJobParams,
};
use crate::mwa_asvo::api::validate::{self, Bounds};
use crate::mwa_asvo::{ENV_GIANT_SQUID_DELIVERY, ENV_GIANT_SQUID_DELIVERY_FORMAT};

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

/// The word that gives a field of the request body the value `null`.
const NULL_WORD: &str = "null";

/// The value of an option for a field that the MWA ASVO schema allows to be
/// `null`: `None` is `null`. The option's schema default is the `Some` value
/// that clap shows in `--help`, and the word `null` gives `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nullable<T>(pub Option<T>);

/// Builds a clap value parser for a field that can be `null` from `inner`,
/// the parser of the field's other values: the word `null` (in any case) is
/// `None`, and anything else is `inner`'s.
pub fn nullable<T>(
    inner: impl Fn(&str) -> Result<T, String> + Clone + Send + Sync + 'static,
) -> impl Fn(&str) -> Result<Nullable<T>, String> + Clone + Send + Sync + 'static {
    move |s: &str| {
        if s.eq_ignore_ascii_case(NULL_WORD) {
            Ok(Nullable(None))
        } else {
            inner(s).map(|v| Nullable(Some(v)))
        }
    }
}

/// The clap value parser of a field that is `true`, `false` or `null`.
pub fn parse_nullable_bool(s: &str) -> Result<Nullable<bool>, String> {
    nullable(|s: &str| {
        s.parse::<bool>()
            .map_err(|_| format!("expected true, false or {NULL_WORD} (got \"{s}\")"))
    })(s)
}

/// The clap value parser of `list --days`: an integer from 1 to 30
/// ([`validate::DAYS`]), as the schema's type.
pub fn parse_days(s: &str) -> Result<NonZeroU64, String> {
    let days = parse_i64_bounds(validate::DAYS)(s)?;
    Ok(validate::days_in_range(days))
}

/// The clap value parser of `list --date-from` and `--date-to`: the
/// library's [`crate::parse_utc_time`], with the error as text.
pub fn parse_utc_time(s: &str) -> Result<jiff::Timestamp, String> {
    crate::parse_utc_time(s).map_err(|e| e.to_string())
}

/// The clap value parser of `--image-size`: one of the MWA ASVO API's fixed
/// image sizes (see [`validate::image_size`]), as the schema's type.
pub fn parse_image_size(s: &str) -> Result<ImageSizes, String> {
    let v: i64 = s.parse().map_err(|e| format!("not a valid integer: {e}"))?;
    validate::image_size(v).map_err(|e| e.to_string())
}

/// The clap value parser of `--nmiter`: an integer within
/// [`validate::NMITER`], as the schema's type.
pub fn parse_nmiter(s: &str) -> Result<NonZeroU64, String> {
    let v = parse_i64_bounds(validate::NMITER)(s)?;
    // The bounds start at 1, so `v` is positive.
    validate::nmiter(v.unsigned_abs()).map_err(|e| e.to_string())
}

/// A request body type of the OpenAPI schema, with its schema defaults. The
/// argument groups below take their clap defaults from it, so that each job
/// type keeps the defaults of its own schema type.
pub trait SchemaDefaults: Clone + std::fmt::Debug + Send + Sync + 'static {
    /// The body with every field at the schema's default.
    fn schema_defaults() -> Self;
}

/// Implement [`SchemaDefaults`] for schema types: each type is paired with
/// the function that gives its defaults.
macro_rules! impl_schema_defaults {
    ($($ty:ty => $defaults:path),+ $(,)?) => {
        $(
            impl SchemaDefaults for $ty {
                fn schema_defaults() -> Self {
                    $defaults()
                }
            }
        )+
    };
}

impl_schema_defaults!(
    DownloadJobParams => download_defaults,
    ConversionJobParams => conversion_defaults,
    ImagingJobFlow1Params => imaging_defaults,
    ImagingJobFlow2Params => image_from_job_defaults,
    VoltageJobParams => voltage_defaults,
);

/// Implement an accessor trait: each method returns that field of the
/// schema type.
macro_rules! impl_fields {
    ($trait:ident for [$($ty:ty),+ $(,)?] $fields:tt) => {
        $(impl_fields!(@one $trait, $ty, $fields);)+
    };
    (@one $trait:ident, $ty:ty, { $($field:ident: $field_ty:ty),+ $(,)? }) => {
        impl $trait for $ty {
            $(
                fn $field(&self) -> $field_ty {
                    self.$field.clone()
                }
            )+
        }
    };
}

/// The delivery fields of a schema type.
pub trait DeliveryFields: SchemaDefaults {
    fn delivery(&self) -> DeliveryDestination;
    fn delivery_format(&self) -> DeliveryFormat;
}

impl_fields!(DeliveryFields for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobFlow1Params,
    ImagingJobFlow2Params,
    VoltageJobParams,
] {
    delivery: DeliveryDestination,
    delivery_format: DeliveryFormat,
});

/// The clap default text of a schema field that the API accepts as null.
/// The schema gives every such field a default that is not null.
///
/// # Panics
///
/// If the schema default is null: the schema and this code disagree.
fn nullable_default<T: ToString>(default: Option<T>, name: &str) -> String {
    default
        .unwrap_or_else(|| panic!("BUG: the schema default of {name} is null"))
        .to_string()
}

/// The preprocessing fields of a schema type (conversion and imaging flow 1).
pub trait PreprocessingFields: SchemaDefaults {
    fn avg_freq_res(&self) -> Option<f64>;
    fn avg_time_res(&self) -> Option<f64>;
    fn flag_edge_width(&self) -> f64;
    fn apply_di_cal(&self) -> bool;
    fn no_apply_amps(&self) -> bool;
    fn centre(&self) -> PhaseCentre;
    fn no_digital_gains(&self) -> Option<bool>;
    fn no_flag_dc(&self) -> Option<bool>;
    fn no_geometric_delay(&self) -> Option<bool>;
    fn no_passband_gains(&self) -> Option<bool>;
    fn no_cable_delay(&self) -> Option<bool>;
    fn no_rfi(&self) -> Option<bool>;
}

impl_fields!(PreprocessingFields for [ConversionJobParams, ImagingJobFlow1Params] {
    avg_freq_res: Option<f64>,
    avg_time_res: Option<f64>,
    flag_edge_width: f64,
    apply_di_cal: bool,
    no_apply_amps: bool,
    centre: PhaseCentre,
    no_digital_gains: Option<bool>,
    no_flag_dc: Option<bool>,
    no_geometric_delay: Option<bool>,
    no_passband_gains: Option<bool>,
    no_cable_delay: Option<bool>,
    no_rfi: Option<bool>,
});

/// The WSClean fields of a schema type (both imaging flows).
pub trait WscleanFields: SchemaDefaults {
    fn apply_primary_beam(&self) -> bool;
    fn auto_mask(&self) -> i64;
    fn auto_threshold(&self) -> f64;
    fn abs_threshold(&self) -> Option<f64>;
    fn channels_out(&self) -> i64;
    fn clean_iterations(&self) -> i64;
    fn clean_threshold(&self) -> Option<f64>;
    fn image_size(&self) -> ImageSizes;
    fn join_channels(&self) -> bool;
    fn join_polarizations(&self) -> bool;
    fn mgain(&self) -> f64;
    fn multiscale(&self) -> bool;
    fn nmiter(&self) -> NonZeroU64;
    fn output_mode(&self) -> ImageOutputMode;
    fn pixel_scale(&self) -> f64;
    fn pol(&self) -> Polarization;
    fn robust(&self) -> f64;
    fn uvw_min(&self) -> f64;
    fn weighting(&self) -> ImageWeighting;
}

impl_fields!(WscleanFields for [ImagingJobFlow1Params, ImagingJobFlow2Params] {
    apply_primary_beam: bool,
    auto_mask: i64,
    auto_threshold: f64,
    abs_threshold: Option<f64>,
    channels_out: i64,
    clean_iterations: i64,
    clean_threshold: Option<f64>,
    image_size: ImageSizes,
    join_channels: bool,
    join_polarizations: bool,
    mgain: f64,
    multiscale: bool,
    nmiter: NonZeroU64,
    output_mode: ImageOutputMode,
    pixel_scale: f64,
    pol: Polarization,
    robust: f64,
    uvw_min: f64,
    weighting: ImageWeighting,
});

/// Implement `apply_to` for an argument group: it sets the fields of a
/// request body from the group's options. The group's fields have the same
/// names as the schema fields. A `nullable` field holds a [`Nullable`] (its
/// `.0` is the schema's `Option`), and a `clone` field is not `Copy`.
macro_rules! impl_apply_to {
    ($args:ident for [$($ty:ty),+ $(,)?] $fields:tt) => {
        $(impl_apply_to!(@one $args, $ty, $fields);)+
    };
    (@one $args:ident, $ty:ty, {
        copy: [$($copy:ident),* $(,)?],
        nullable: [$($nullable:ident),* $(,)?],
        clone: [$($clone:ident),* $(,)?] $(,)?
    }) => {
        impl $args<$ty> {
            /// Set the fields of `params` that these options cover.
            pub fn apply_to(&self, params: &mut $ty) {
                $(params.$copy = self.$copy;)*
                $(params.$nullable = self.$nullable.0;)*
                $(params.$clone = self.$clone.clone();)*
            }
        }
    };
}

/// The `allow_resubmit` field of a schema type (every job type has it).
pub trait ResubmitField: SchemaDefaults {
    fn allow_resubmit(&self) -> Option<bool>;
}

impl_fields!(ResubmitField for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobFlow1Params,
    ImagingJobFlow2Params,
    VoltageJobParams,
] {
    allow_resubmit: Option<bool>,
});

// The argument groups below are generic over the schema type, so their
// defaults use `default_value` with a `String`, never `default_value_t`:
// clap's `default_value_t` keeps the text in a `static` of the generic
// function, which all the schema types share, so every group would show
// the defaults of the first type used.

/// The clap options of [`DeliveryArgs`], as clap reads them. [`DeliveryArgs`]
/// adds the check that the delivery and its format agree.
#[derive(clap::Args, Debug, Clone)]
struct RawDeliveryArgs<P: DeliveryFields> {
    /// Tell MWA ASVO where to deliver the data.
    #[arg(short, long, default_value = P::schema_defaults().delivery().to_string(), env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<DeliveryDestination>::new())]
    delivery: DeliveryDestination,

    /// Tell MWA ASVO to deliver the data in a particular format. Must be tar
    /// when the delivery is acacia.
    #[arg(short = 'f', long, default_value = P::schema_defaults().delivery_format().to_string(), env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    delivery_format: DeliveryFormat,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

/// Where and how the MWA ASVO delivers a job's files. The defaults are those
/// of the schema type `P`. The options are `--delivery` and `--delivery-format`; a
/// `delivery_format` of `files` with a `delivery` of `acacia` is a usage
/// error ([`validate::check_delivery_format`], the library's check), whether
/// the values come from the command line or the environment.
#[derive(Debug, Clone)]
pub struct DeliveryArgs<P: DeliveryFields> {
    pub delivery: DeliveryDestination,
    pub delivery_format: DeliveryFormat,
    schema: PhantomData<fn() -> P>,
}

impl<P: DeliveryFields> clap::Args for DeliveryArgs<P> {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        <RawDeliveryArgs<P> as clap::Args>::augment_args(cmd)
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        <RawDeliveryArgs<P> as clap::Args>::augment_args_for_update(cmd)
    }
}

impl<P: DeliveryFields> clap::FromArgMatches for DeliveryArgs<P> {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let raw = <RawDeliveryArgs<P> as clap::FromArgMatches>::from_arg_matches(matches)?;
        validate::check_delivery_format(raw.delivery, raw.delivery_format)
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::ArgumentConflict, e))?;
        Ok(Self {
            delivery: raw.delivery,
            delivery_format: raw.delivery_format,
            schema: PhantomData,
        })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

/// The preprocessing arguments of the conversion and imaging (flow 1) jobs.
/// The defaults are those of the schema type `P`.
#[derive(clap::Args, Debug, Clone)]
pub struct PreprocessingArgs<P: PreprocessingFields> {
    /// Frequency resolution to average to (kHz), or null.
    #[arg(long, default_value = nullable_default(P::schema_defaults().avg_freq_res(), "avg_freq_res"), value_parser = nullable(parse_f64_bounds(validate::AVG_FREQ_RES)))]
    pub avg_freq_res: Nullable<f64>,

    /// Time resolution to average to (s), or null.
    #[arg(long, default_value = nullable_default(P::schema_defaults().avg_time_res(), "avg_time_res"), value_parser = nullable(parse_f64_bounds(validate::AVG_TIME_RES)))]
    pub avg_time_res: Nullable<f64>,

    /// Width of frequency edge flagging (kHz).
    #[arg(long, default_value = P::schema_defaults().flag_edge_width().to_string(), value_parser = parse_f64_bounds(validate::FLAG_EDGE_WIDTH))]
    pub flag_edge_width: f64,

    /// Whether to apply the DI calibration solution.
    #[arg(
        long,
        default_value = P::schema_defaults().apply_di_cal().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_di_cal: bool,

    /// Phase centre: "phase", "pointing", or "custom".
    /// If "custom", also supply --custom-centre-ra and --custom-centre-dec.
    #[arg(long, alias = "phase-center", default_value = P::schema_defaults().centre().to_string(), value_parser = SchemaEnumParser::<PhaseCentre>::new())]
    pub centre: PhaseCentre,

    /// Custom phase centre right ascension (degrees). Requires --centre custom.
    #[arg(long, aliases = ["phase-centre-ra", "custom-ra"], value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_RA))]
    pub custom_centre_ra: Option<f64>,

    /// Custom phase centre declination (degrees). Requires --centre custom.
    #[arg(long, aliases = ["phase-centre-dec", "custom-dec"], value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_DEC))]
    pub custom_centre_dec: Option<f64>,

    /// Whether to skip applying amplitude calibration solutions.
    #[arg(
        long,
        default_value = P::schema_defaults().no_apply_amps().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub no_apply_amps: bool,

    /// Whether to skip applying digital gains. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_digital_gains(), "no_digital_gains"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_digital_gains: Nullable<bool>,

    /// Whether to skip flagging the DC channel. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_flag_dc(), "no_flag_dc"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_flag_dc: Nullable<bool>,

    /// Whether to skip applying geometric delay corrections. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_geometric_delay(), "no_geometric_delay"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_geometric_delay: Nullable<bool>,

    /// Whether to skip applying passband gain corrections. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_passband_gains(), "no_passband_gains"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_passband_gains: Nullable<bool>,

    /// Whether to skip applying cable delay corrections. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_cable_delay(), "no_cable_delay"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_cable_delay: Nullable<bool>,

    /// Whether to skip RFI flagging. Takes true, false or null.
    #[arg(
        long,
        default_value = nullable_default(P::schema_defaults().no_rfi(), "no_rfi"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_rfi: Nullable<bool>,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

/// The WSClean arguments of both imaging jobs. The defaults are those of
/// the schema type `P`.
#[derive(clap::Args, Debug, Clone)]
pub struct WscleanArgs<P: WscleanFields> {
    /// Whether to apply the primary beam correction.
    #[arg(
        long,
        default_value = P::schema_defaults().apply_primary_beam().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_primary_beam: bool,

    /// WSClean -auto-mask value.
    #[arg(long, default_value = P::schema_defaults().auto_mask().to_string(), value_parser = parse_i64_bounds(validate::AUTO_MASK))]
    pub auto_mask: i64,

    /// WSClean -auto-threshold value.
    #[arg(long, default_value = P::schema_defaults().auto_threshold().to_string(), value_parser = parse_f64_bounds(validate::AUTO_THRESHOLD))]
    pub auto_threshold: f64,

    /// Absolute cleaning threshold (Jy), or null.
    #[arg(long, default_value = nullable_default(P::schema_defaults().abs_threshold(), "abs_threshold"), value_parser = nullable(parse_f64_bounds(validate::ABS_THRESHOLD)))]
    pub abs_threshold: Nullable<f64>,

    /// Number of output channel groups.
    #[arg(long, default_value = P::schema_defaults().channels_out().to_string())]
    pub channels_out: i64,

    /// WSClean -niter value (max clean iterations).
    #[arg(long, default_value = P::schema_defaults().clean_iterations().to_string(), value_parser = parse_i64_bounds(validate::CLEAN_ITERATIONS))]
    pub clean_iterations: i64,

    /// WSClean cleaning threshold (Jy), or null.
    #[arg(long, default_value = nullable_default(P::schema_defaults().clean_threshold(), "clean_threshold"), value_parser = nullable(parse_f64_bounds(validate::CLEAN_THRESHOLD)))]
    pub clean_threshold: Nullable<f64>,

    /// WSClean image size in pixels.
    #[arg(long, default_value = (*P::schema_defaults().image_size()).to_string(), value_parser = parse_image_size)]
    pub image_size: ImageSizes,

    /// Join output channel groups for cleaning.
    #[arg(
        long,
        default_value = P::schema_defaults().join_channels().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_channels: bool,

    /// Join polarisations for cleaning.
    #[arg(
        long,
        default_value = P::schema_defaults().join_polarizations().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_polarizations: bool,

    /// WSClean -mgain value.
    #[arg(long, default_value = P::schema_defaults().mgain().to_string(), value_parser = parse_f64_bounds(validate::MGAIN))]
    pub mgain: f64,

    /// Enable WSClean multiscale cleaning.
    #[arg(
        long,
        default_value = P::schema_defaults().multiscale().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub multiscale: bool,

    /// WSClean -nmiter value (max major cleaning iterations).
    #[arg(long, default_value = P::schema_defaults().nmiter().get().to_string(), value_parser = parse_nmiter)]
    pub nmiter: NonZeroU64,

    /// Number of w-projection layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::NWLAYERS))]
    pub nwlayers: Option<i64>,

    /// The output mode / product to request.
    #[arg(short = 'o', long, default_value = P::schema_defaults().output_mode().to_string(), value_parser = SchemaEnumParser::<ImageOutputMode>::new())]
    pub output_mode: ImageOutputMode,

    /// Pixel scale (arcsec/pixel).
    #[arg(long, default_value = P::schema_defaults().pixel_scale().to_string(), value_parser = parse_f64_bounds(validate::PIXEL_SCALE))]
    pub pixel_scale: f64,

    /// Polarisation to image: XX, YY or XXYY.
    #[arg(long, default_value = P::schema_defaults().pol().to_string(), value_parser = SchemaEnumParser::<Polarization>::new())]
    pub pol: Polarization,

    /// WSClean -robust (Briggs robustness) value.
    #[arg(long, default_value = P::schema_defaults().robust().to_string(), value_parser = parse_f64_bounds(validate::ROBUST))]
    pub robust: f64,

    /// Maximum uv distance to image, in wavelengths (upper bound on
    /// the range that can be requested).
    #[arg(long, value_parser = parse_f64_bounds(validate::UVW_MAX))]
    pub uvw_max: Option<f64>,

    /// Minimum uv distance to image, in wavelengths.
    #[arg(long, default_value = P::schema_defaults().uvw_min().to_string(), value_parser = parse_f64_bounds(validate::UVW_MIN))]
    pub uvw_min: f64,

    /// WSClean weighting scheme.
    #[arg(long, default_value = P::schema_defaults().weighting().to_string(), value_parser = SchemaEnumParser::<ImageWeighting>::new())]
    pub weighting: ImageWeighting,

    /// Number of w-stacking layers. Leave unset to let the server
    /// decide.
    #[arg(long, value_parser = parse_i64_bounds(validate::WSTACK_NWLAYERS))]
    pub wstack_nwlayers: Option<i64>,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

/// The `--allow-resubmit` option, which every submit command has. The
/// default is that of the schema type `P`.
#[derive(clap::Args, Debug, Clone)]
pub struct ResubmitArgs<P: ResubmitField> {
    /// Allow resubmitting a job even if an identical one has completed.
    /// Takes true, false or null.
    #[arg(
        short = 'r',
        long,
        default_value = nullable_default(P::schema_defaults().allow_resubmit(), "allow_resubmit"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub allow_resubmit: Nullable<bool>,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

impl_apply_to!(DeliveryArgs for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobFlow1Params,
    ImagingJobFlow2Params,
    VoltageJobParams,
] {
    copy: [delivery, delivery_format],
    nullable: [],
    clone: [],
});

impl_apply_to!(ResubmitArgs for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobFlow1Params,
    ImagingJobFlow2Params,
    VoltageJobParams,
] {
    copy: [],
    nullable: [allow_resubmit],
    clone: [],
});

impl_apply_to!(PreprocessingArgs for [ConversionJobParams, ImagingJobFlow1Params] {
    copy: [
        flag_edge_width,
        apply_di_cal,
        centre,
        custom_centre_ra,
        custom_centre_dec,
        no_apply_amps,
    ],
    nullable: [
        avg_freq_res,
        avg_time_res,
        no_digital_gains,
        no_flag_dc,
        no_geometric_delay,
        no_passband_gains,
        no_cable_delay,
        no_rfi,
    ],
    clone: [],
});

impl_apply_to!(WscleanArgs for [ImagingJobFlow1Params, ImagingJobFlow2Params] {
    copy: [
        apply_primary_beam,
        auto_mask,
        auto_threshold,
        channels_out,
        clean_iterations,
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
    ],
    nullable: [abs_threshold, clean_threshold],
    clone: [image_size],
});

/// Arguments shared by the visibility and metadata download jobs, which use
/// the same request body and the same endpoint, differing only in their
/// `download_type`. (The beamformer job uses the same body too, with
/// `download_type` `beam`: see [`BeamformerJobArgs`].)
#[derive(clap::Args, Debug, Clone)]
pub struct DownloadJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<DownloadJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<DownloadJobParams>,
}

impl DownloadJobArgs {
    /// The request body for a raw visibility download job.
    pub fn to_vis_params(&self, obs_id: ObsId) -> DownloadJobParams {
        self.to_params(obs_id, DownloadType::Vis)
    }

    /// The request body for a metadata download job.
    pub fn to_meta_params(&self, obs_id: ObsId) -> DownloadJobParams {
        self.to_params(obs_id, DownloadType::Meta)
    }

    /// The request body for a download job of `download_type`. The CLI has a
    /// value for every option (its defaults are the schema's), so each field
    /// that the options cover is set.
    fn to_params(&self, obs_id: ObsId, download_type: DownloadType) -> DownloadJobParams {
        let mut params = download_params(obs_id, download_type);
        self.delivery_args.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for a preprocessing/conversion job.
#[derive(clap::Args, Debug, Clone)]
pub struct ConversionJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ConversionJobParams>,

    /// Output format: "ms" (measurement set) or "uvfits".
    #[arg(short = 'o', long, default_value_t = conversion_defaults().output, value_parser = SchemaEnumParser::<OutputFormat>::new())]
    pub output: OutputFormat,

    #[command(flatten)]
    pub preprocessing: PreprocessingArgs<ConversionJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ConversionJobParams>,
}

impl ConversionJobArgs {
    /// The request body for a conversion job for a single Obs ID. The CLI has
    /// a value for every option (its defaults are the schema's), so each
    /// field that the options cover is set.
    pub fn to_params(&self, obs_id: ObsId) -> ConversionJobParams {
        let mut params = conversion_params(obs_id);
        self.delivery_args.apply_to(&mut params);
        params.output = self.output;
        self.preprocessing.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for an imaging job that starts from raw visibilities (flow 1).
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ImagingJobFlow1Params>,

    #[command(flatten)]
    pub preprocessing: PreprocessingArgs<ImagingJobFlow1Params>,

    #[command(flatten)]
    pub wsclean: WscleanArgs<ImagingJobFlow1Params>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ImagingJobFlow1Params>,
}

impl ImagingJobArgs {
    /// The request body for an imaging job for a single Obs ID. The CLI has a
    /// value for every option (its defaults are the schema's), so each field
    /// that the options cover is set.
    pub fn to_params(&self, obs_id: ObsId) -> ImagingJobFlow1Params {
        let mut params = imaging_params(obs_id);
        self.delivery_args.apply_to(&mut params);
        self.preprocessing.apply_to(&mut params);
        self.wsclean.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for an imaging job that starts from an existing conversion job
/// (flow 2).
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingFromJobArgs {
    /// The MWA ASVO conversion Job ID to image from. Required.
    #[arg(long)]
    pub source_job_id: NonZeroU64,

    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ImagingJobFlow2Params>,

    #[command(flatten)]
    pub wsclean: WscleanArgs<ImagingJobFlow2Params>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ImagingJobFlow2Params>,
}

impl ImagingFromJobArgs {
    /// The request body for an image of the conversion job
    /// `--source-job-id`. The CLI has a value for every option (its defaults
    /// are the schema's), so each field that the options cover is set.
    /// `obs_id` is left out (`null`): the MWA ASVO takes it from the source job.
    pub fn to_params(&self) -> ImagingJobFlow2Params {
        let mut params = image_from_job_params(self.source_job_id);
        self.delivery_args.apply_to(&mut params);
        self.wsclean.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for a voltage download job.
#[derive(clap::Args, Debug, Clone)]
pub struct VoltageJobArgs {
    /// The MWA ASVO delivers a voltage job to scratch, as files, and the
    /// library refuses any other `--delivery` or `--delivery-format`.
    /// Scratch requires the "mwavcs" Pawsey Group on your MWA ASVO profile.
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<VoltageJobParams>,

    /// The offset in seconds from the start GPS time of the observation.
    #[arg(short, long, value_parser = parse_i64_bounds(validate::VOLTAGE_OFFSET))]
    pub offset: i64,

    /// The duration (in seconds) to download.
    #[arg(short = 'u', long)]
    pub duration: u64,

    /// The 'from' receiver channel number (0-255).
    #[arg(long)]
    pub from_channel: Option<u8>,

    /// The 'to' receiver channel number (0-255).
    #[arg(long)]
    pub to_channel: Option<u8>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<VoltageJobParams>,
}

impl VoltageJobArgs {
    /// The request body for a voltage job.
    pub fn to_params(&self, obs_id: ObsId) -> VoltageJobParams {
        let mut params = VoltageJobParams {
            from_channel: self.from_channel,
            to_channel: self.to_channel,
            ..voltage_params(obs_id, self.offset, self.duration)
        };
        self.delivery_args.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for a beamformer download job. It is a download job (see
/// [`DownloadJobArgs`]) with `download_type` `beam`.
#[derive(clap::Args, Debug, Clone)]
pub struct BeamformerJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<DownloadJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<DownloadJobParams>,
}

impl BeamformerJobArgs {
    /// The request body for a beamformer job.
    pub fn to_params(&self, obs_id: ObsId) -> DownloadJobParams {
        let mut params = download_params(obs_id, DownloadType::Beam);
        self.delivery_args.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}
