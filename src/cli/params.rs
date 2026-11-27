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
};
use crate::mwa_asvo::api::openapi::cmd_help::{self, download_job_params as download_help};
use crate::mwa_asvo::api::openapi::{
    ConversionJobParams, DeliveryDestination, DeliveryFormat, DownloadJobParams,
    ImageFromJobParams, ImageOutputMode, ImageSizes, ImageWeighting, ImagingJobParams,
    JobsByUserRequest, OutputFormat, PhaseCentre, Polarization,
};
use crate::mwa_asvo::api::validate::{self, Bounds};
use crate::mwa_asvo::{
    job_field_names, parse_job_field, ENV_GIANT_SQUID_DELIVERY, ENV_GIANT_SQUID_DELIVERY_FORMAT,
};

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

/// The help text `text` of an option whose value must be within `bounds`,
/// with the range after it in the style of clap's `[default: ...]`: for
/// example `<text> [range: 0 to 7200]`. The bounds are the library's
/// schema limits, so the help cannot drift from the API.
fn help_with_range(text: &str, bounds: Bounds) -> String {
    format!("{text} [range: {}]", bounds.range_text())
}

/// The range of a receiver channel number (`from_channel`, `to_channel`):
/// the range of its type, `u8`, which the validate tests check is the
/// schema's limit.
const RECEIVER_CHANNEL_RANGE: Bounds = Bounds::between(u8::MIN as f64, u8::MAX as f64);

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
    parse_positive_bounds(validate::DAYS, s)
}

/// Parse `s` as an integer within `bounds`, whose lower limit is 1, as the
/// schema's type.
///
/// # Panics
///
/// If `bounds` let a value below 1 through: the bounds are wrong.
fn parse_positive_bounds(bounds: Bounds, s: &str) -> Result<NonZeroU64, String> {
    let v = parse_i64_bounds(bounds)(s)?;
    // The bounds start at 1, so `v` is positive.
    Ok(NonZeroU64::new(v.unsigned_abs()).expect("BUG: the bounds let 0 through"))
}

/// The clap value parser of `list --fields`: one name of the library's
/// [`job_field_names`].
pub fn parse_field(s: &str) -> Result<String, String> {
    parse_job_field(s).map(str::to_string)
}

/// The long help of `list --fields`, with every field name.
pub fn fields_long_help() -> String {
    format!(
        "Add a column for each of these job fields (comma separated), after \
         the other columns. A job that does not have a field (for example a \
         visibility job has no avg_time_res) has an empty cell, and a field \
         with no value is `null`. The fields of a job's params are the \
         schema's names. A key that the MWA ASVO did not send for a job has \
         the schema's default. The case, hyphens and underscores do not \
         matter. Cannot be used with --json, --legacy-json or --ids.\n\n\
         Fields: {}",
        job_field_names().join(", ")
    )
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

/// The clap value parser of `--channels-out`: an integer within
/// [`validate::CHANNELS_OUT`], as the schema's type.
pub fn parse_channels_out(s: &str) -> Result<NonZeroU64, String> {
    parse_positive_bounds(validate::CHANNELS_OUT, s)
}

/// The clap value parser of `--nmiter`: an integer within
/// [`validate::NMITER`], as the schema's type.
pub fn parse_nmiter(s: &str) -> Result<NonZeroU64, String> {
    parse_positive_bounds(validate::NMITER, s)
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
    ImagingJobParams => imaging_defaults,
    ImageFromJobParams => image_from_job_defaults,
);

/// Implement a help trait: each constant is the schema's command-line help
/// text (`x-cmd-help`) of the field of that name of the schema type, from
/// the type's module of the generated [`cmd_help`].
macro_rules! impl_help {
    ($trait:ident for [$($ty:ty => $module:ident),+ $(,)?] $constants:tt) => {
        $(impl_help!(@one $trait, $ty, $module, $constants);)+
    };
    (@one $trait:ident, $ty:ty, $module:ident, { $($constant:ident),+ $(,)? }) => {
        impl $trait for $ty {
            $(const $constant: &'static str = cmd_help::$module::$constant;)+
        }
    };
}

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

/// The help texts of the delivery options of a schema type.
pub trait DeliveryHelp {
    const DELIVERY: &'static str;
    const DELIVERY_FORMAT: &'static str;
}

impl_help!(DeliveryHelp for [
    DownloadJobParams => download_job_params,
    ConversionJobParams => conversion_job_params,
    ImagingJobParams => imaging_job_params,
    ImageFromJobParams => image_from_job_params,
] { DELIVERY, DELIVERY_FORMAT });

/// The delivery fields of a schema type.
pub trait DeliveryFields: SchemaDefaults + DeliveryHelp {
    fn delivery(&self) -> DeliveryDestination;
    fn delivery_format(&self) -> DeliveryFormat;
}

impl_fields!(DeliveryFields for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobParams,
    ImageFromJobParams,
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

/// The help texts of the preprocessing options of a schema type.
pub trait PreprocessingHelp {
    const AVG_FREQ_RES: &'static str;
    const AVG_TIME_RES: &'static str;
    const FLAG_EDGE_WIDTH: &'static str;
    const APPLY_DI_CAL: &'static str;
    const CENTRE: &'static str;
    const CUSTOM_CENTRE_RA: &'static str;
    const CUSTOM_CENTRE_DEC: &'static str;
    const NO_APPLY_AMPS: &'static str;
    const NO_DIGITAL_GAINS: &'static str;
    const NO_FLAG_DC: &'static str;
    const NO_GEOMETRIC_DELAY: &'static str;
    const NO_PASSBAND_GAINS: &'static str;
    const NO_CABLE_DELAY: &'static str;
    const NO_RFI: &'static str;
}

impl_help!(PreprocessingHelp for [
    ConversionJobParams => conversion_job_params,
    ImagingJobParams => imaging_job_params,
] {
    AVG_FREQ_RES, AVG_TIME_RES, FLAG_EDGE_WIDTH, APPLY_DI_CAL, CENTRE, CUSTOM_CENTRE_RA,
    CUSTOM_CENTRE_DEC, NO_APPLY_AMPS, NO_DIGITAL_GAINS, NO_FLAG_DC, NO_GEOMETRIC_DELAY,
    NO_PASSBAND_GAINS, NO_CABLE_DELAY, NO_RFI,
});

/// The preprocessing fields of a schema type (conversion, and imaging from an
/// Obs ID).
pub trait PreprocessingFields: SchemaDefaults + PreprocessingHelp {
    fn avg_freq_res(&self) -> Option<f64>;
    fn avg_time_res(&self) -> Option<f64>;
    fn flag_edge_width(&self) -> Option<f64>;
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

impl_fields!(PreprocessingFields for [ConversionJobParams, ImagingJobParams] {
    avg_freq_res: Option<f64>,
    avg_time_res: Option<f64>,
    flag_edge_width: Option<f64>,
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

/// The help texts of the WSClean options of a schema type.
pub trait WscleanHelp {
    const APPLY_PRIMARY_BEAM: &'static str;
    const AUTO_MASK: &'static str;
    const AUTO_THRESHOLD: &'static str;
    const ABS_THRESHOLD: &'static str;
    const CHANNELS_OUT: &'static str;
    const CLEAN_ITERATIONS: &'static str;
    const IMAGE_SIZE: &'static str;
    const JOIN_CHANNELS: &'static str;
    const JOIN_POLARIZATIONS: &'static str;
    const MGAIN: &'static str;
    const MULTISCALE: &'static str;
    const NMITER: &'static str;
    const OUTPUT_MODE: &'static str;
    const PIXEL_SCALE: &'static str;
    const POLARIZATION: &'static str;
    const ROBUST: &'static str;
    const UV_MAX: &'static str;
    const UV_MIN: &'static str;
    const WEIGHTING: &'static str;
    const WSTACK_NWLAYERS: &'static str;
}

impl_help!(WscleanHelp for [
    ImagingJobParams => imaging_job_params,
    ImageFromJobParams => image_from_job_params,
] {
    APPLY_PRIMARY_BEAM, AUTO_MASK, AUTO_THRESHOLD, ABS_THRESHOLD, CHANNELS_OUT, CLEAN_ITERATIONS,
    IMAGE_SIZE, JOIN_CHANNELS, JOIN_POLARIZATIONS, MGAIN, MULTISCALE, NMITER, OUTPUT_MODE,
    PIXEL_SCALE, POLARIZATION, ROBUST, UV_MAX, UV_MIN, WEIGHTING, WSTACK_NWLAYERS,
});

/// The WSClean fields of a schema type (both imaging bodies).
pub trait WscleanFields: SchemaDefaults + WscleanHelp {
    fn apply_primary_beam(&self) -> bool;
    fn auto_mask(&self) -> i64;
    fn auto_threshold(&self) -> f64;
    fn abs_threshold(&self) -> Option<f64>;
    fn channels_out(&self) -> NonZeroU64;
    fn clean_iterations(&self) -> i64;
    fn join_channels(&self) -> bool;
    fn join_polarizations(&self) -> bool;
    fn mgain(&self) -> f64;
    fn multiscale(&self) -> bool;
    fn nmiter(&self) -> NonZeroU64;
    fn polarization(&self) -> Polarization;
    fn robust(&self) -> f64;
    fn uv_min(&self) -> f64;
    fn weighting(&self) -> ImageWeighting;
}

impl_fields!(WscleanFields for [ImagingJobParams, ImageFromJobParams] {
    apply_primary_beam: bool,
    auto_mask: i64,
    auto_threshold: f64,
    abs_threshold: Option<f64>,
    channels_out: NonZeroU64,
    clean_iterations: i64,
    join_channels: bool,
    join_polarizations: bool,
    mgain: f64,
    multiscale: bool,
    nmiter: NonZeroU64,
    polarization: Polarization,
    robust: f64,
    uv_min: f64,
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

/// The help text of the `--allow-resubmit` option of a schema type.
pub trait ResubmitHelp {
    const ALLOW_RESUBMIT: &'static str;
}

impl_help!(ResubmitHelp for [
    DownloadJobParams => download_job_params,
    ConversionJobParams => conversion_job_params,
    ImagingJobParams => imaging_job_params,
    ImageFromJobParams => image_from_job_params,
] { ALLOW_RESUBMIT });

/// The `allow_resubmit` field of a schema type (every job type has it).
pub trait ResubmitField: SchemaDefaults + ResubmitHelp {
    fn allow_resubmit(&self) -> Option<bool>;
}

impl_fields!(ResubmitField for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobParams,
    ImageFromJobParams,
] {
    allow_resubmit: Option<bool>,
});

// The argument groups below are generic over the schema type, so their
// defaults use `default_value` with a `String`, never `default_value_t`:
// clap's `default_value_t` keeps the text in a `static` of the generic
// function, which all the schema types share, so every group would show
// the defaults of the first type used.

/// Where and how the MWA ASVO delivers a job's files. The defaults are those
/// of the schema type `P`. The options are `--delivery` and `--delivery-format`.
/// Which formats a delivery takes is the MWA ASVO's rule: it refuses a job
/// whose delivery and format do not agree.
#[derive(clap::Args, Debug, Clone)]
pub struct DeliveryArgs<P: DeliveryFields> {
    #[arg(help = P::DELIVERY, short, long, default_value = P::schema_defaults().delivery().to_string(), env = ENV_GIANT_SQUID_DELIVERY, value_parser = SchemaEnumParser::<DeliveryDestination>::new())]
    pub delivery: DeliveryDestination,

    #[arg(help = P::DELIVERY_FORMAT, short = 'f', long, default_value = P::schema_defaults().delivery_format().to_string(), env = ENV_GIANT_SQUID_DELIVERY_FORMAT, value_parser = SchemaEnumParser::<DeliveryFormat>::new())]
    pub delivery_format: DeliveryFormat,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

impl<P: DeliveryFields> DeliveryArgs<P> {
    /// The delivery and its format at the defaults of the schema type `P`,
    /// for a command that has no delivery options.
    pub fn schema_defaults() -> Self {
        let defaults = P::schema_defaults();
        Self {
            delivery: defaults.delivery(),
            delivery_format: defaults.delivery_format(),
            schema: PhantomData,
        }
    }
}

/// The preprocessing arguments of the conversion and imaging (from an Obs ID) jobs.
/// The defaults are those of the schema type `P`.
#[derive(clap::Args, Debug, Clone)]
pub struct PreprocessingArgs<P: PreprocessingFields> {
    #[arg(help = help_with_range(P::AVG_FREQ_RES, validate::AVG_FREQ_RES), long, default_value = nullable_default(P::schema_defaults().avg_freq_res(), "avg_freq_res"), value_parser = nullable(parse_f64_bounds(validate::AVG_FREQ_RES)))]
    pub avg_freq_res: Nullable<f64>,

    #[arg(help = help_with_range(P::AVG_TIME_RES, validate::AVG_TIME_RES), long, default_value = nullable_default(P::schema_defaults().avg_time_res(), "avg_time_res"), value_parser = nullable(parse_f64_bounds(validate::AVG_TIME_RES)))]
    pub avg_time_res: Nullable<f64>,

    #[arg(help = help_with_range(P::FLAG_EDGE_WIDTH, validate::FLAG_EDGE_WIDTH), long, default_value = nullable_default(P::schema_defaults().flag_edge_width(), "flag_edge_width"), value_parser = nullable(parse_f64_bounds(validate::FLAG_EDGE_WIDTH)))]
    pub flag_edge_width: Nullable<f64>,

    #[arg(
        help = P::APPLY_DI_CAL,
        long,
        default_value = P::schema_defaults().apply_di_cal().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_di_cal: bool,

    #[arg(help = P::CENTRE, long, alias = "phase-center", default_value = P::schema_defaults().centre().to_string(), value_parser = SchemaEnumParser::<PhaseCentre>::new())]
    pub centre: PhaseCentre,

    #[arg(help = help_with_range(P::CUSTOM_CENTRE_RA, validate::CUSTOM_CENTRE_RA), long, aliases = ["phase-centre-ra", "custom-ra"], value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_RA))]
    pub custom_centre_ra: Option<f64>,

    #[arg(help = help_with_range(P::CUSTOM_CENTRE_DEC, validate::CUSTOM_CENTRE_DEC), long, aliases = ["phase-centre-dec", "custom-dec"], value_parser = parse_f64_bounds(validate::CUSTOM_CENTRE_DEC))]
    pub custom_centre_dec: Option<f64>,

    #[arg(
        help = P::NO_APPLY_AMPS,
        long,
        default_value = P::schema_defaults().no_apply_amps().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub no_apply_amps: bool,

    #[arg(
        help = P::NO_DIGITAL_GAINS,
        long,
        default_value = nullable_default(P::schema_defaults().no_digital_gains(), "no_digital_gains"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_digital_gains: Nullable<bool>,

    #[arg(
        help = P::NO_FLAG_DC,
        long,
        default_value = nullable_default(P::schema_defaults().no_flag_dc(), "no_flag_dc"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_flag_dc: Nullable<bool>,

    #[arg(
        help = P::NO_GEOMETRIC_DELAY,
        long,
        default_value = nullable_default(P::schema_defaults().no_geometric_delay(), "no_geometric_delay"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_geometric_delay: Nullable<bool>,

    #[arg(
        help = P::NO_PASSBAND_GAINS,
        long,
        default_value = nullable_default(P::schema_defaults().no_passband_gains(), "no_passband_gains"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_passband_gains: Nullable<bool>,

    #[arg(
        help = P::NO_CABLE_DELAY,
        long,
        default_value = nullable_default(P::schema_defaults().no_cable_delay(), "no_cable_delay"),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
        value_parser = parse_nullable_bool,
    )]
    pub no_cable_delay: Nullable<bool>,

    #[arg(
        help = P::NO_RFI,
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
    #[arg(
        help = P::APPLY_PRIMARY_BEAM,
        long,
        default_value = P::schema_defaults().apply_primary_beam().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub apply_primary_beam: bool,

    #[arg(help = help_with_range(P::AUTO_MASK, validate::AUTO_MASK), long, default_value = P::schema_defaults().auto_mask().to_string(), value_parser = parse_i64_bounds(validate::AUTO_MASK))]
    pub auto_mask: i64,

    #[arg(help = help_with_range(P::AUTO_THRESHOLD, validate::AUTO_THRESHOLD), long, default_value = P::schema_defaults().auto_threshold().to_string(), value_parser = parse_f64_bounds(validate::AUTO_THRESHOLD))]
    pub auto_threshold: f64,

    #[arg(help = help_with_range(P::ABS_THRESHOLD, validate::ABS_THRESHOLD), long, default_value = nullable_default(P::schema_defaults().abs_threshold(), "abs_threshold"), value_parser = nullable(parse_f64_bounds(validate::ABS_THRESHOLD)))]
    pub abs_threshold: Nullable<f64>,

    #[arg(help = help_with_range(P::CHANNELS_OUT, validate::CHANNELS_OUT), long, default_value = P::schema_defaults().channels_out().get().to_string(), value_parser = parse_channels_out)]
    pub channels_out: NonZeroU64,

    #[arg(help = help_with_range(P::CLEAN_ITERATIONS, validate::CLEAN_ITERATIONS), long, default_value = P::schema_defaults().clean_iterations().to_string(), value_parser = parse_i64_bounds(validate::CLEAN_ITERATIONS))]
    pub clean_iterations: i64,

    #[arg(help = P::IMAGE_SIZE, long, value_parser = parse_image_size)]
    pub image_size: Option<ImageSizes>,

    #[arg(
        help = P::JOIN_CHANNELS,
        long,
        default_value = P::schema_defaults().join_channels().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_channels: bool,

    #[arg(
        help = P::JOIN_POLARIZATIONS,
        long,
        default_value = P::schema_defaults().join_polarizations().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub join_polarizations: bool,

    #[arg(help = help_with_range(P::MGAIN, validate::MGAIN), long, default_value = P::schema_defaults().mgain().to_string(), value_parser = parse_f64_bounds(validate::MGAIN))]
    pub mgain: f64,

    #[arg(
        help = P::MULTISCALE,
        long,
        default_value = P::schema_defaults().multiscale().to_string(),
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub multiscale: bool,

    #[arg(help = help_with_range(P::NMITER, validate::NMITER), long, default_value = P::schema_defaults().nmiter().get().to_string(), value_parser = parse_nmiter)]
    pub nmiter: NonZeroU64,

    #[arg(help = P::OUTPUT_MODE, short = 'o', long, value_parser = SchemaEnumParser::<ImageOutputMode>::new())]
    pub output_mode: ImageOutputMode,

    #[arg(help = help_with_range(P::PIXEL_SCALE, validate::PIXEL_SCALE), long, value_parser = parse_f64_bounds(validate::PIXEL_SCALE))]
    pub pixel_scale: Option<f64>,

    #[arg(help = P::POLARIZATION, long, default_value = P::schema_defaults().polarization().to_string(), value_parser = SchemaEnumParser::<Polarization>::new())]
    pub polarization: Polarization,

    #[arg(help = help_with_range(P::ROBUST, validate::ROBUST), long, default_value = P::schema_defaults().robust().to_string(), value_parser = parse_f64_bounds(validate::ROBUST))]
    pub robust: f64,

    #[arg(help = help_with_range(P::UV_MAX, validate::UV_MAX), long, value_parser = parse_f64_bounds(validate::UV_MAX))]
    pub uv_max: Option<f64>,

    #[arg(help = help_with_range(P::UV_MIN, validate::UV_MIN), long, default_value = P::schema_defaults().uv_min().to_string(), value_parser = parse_f64_bounds(validate::UV_MIN))]
    pub uv_min: f64,

    #[arg(help = P::WEIGHTING, long, default_value = P::schema_defaults().weighting().to_string(), value_parser = SchemaEnumParser::<ImageWeighting>::new())]
    pub weighting: ImageWeighting,

    #[arg(help = help_with_range(P::WSTACK_NWLAYERS, validate::WSTACK_NWLAYERS), long, value_parser = parse_i64_bounds(validate::WSTACK_NWLAYERS))]
    pub wstack_nwlayers: Option<i64>,

    #[arg(skip)]
    schema: PhantomData<fn() -> P>,
}

/// The `--allow-resubmit` option, which every submit command has. The
/// default is that of the schema type `P`.
#[derive(clap::Args, Debug, Clone)]
pub struct ResubmitArgs<P: ResubmitField> {
    #[arg(
        help = P::ALLOW_RESUBMIT,
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
    ImagingJobParams,
    ImageFromJobParams,
] {
    copy: [delivery, delivery_format],
    nullable: [],
    clone: [],
});

impl_apply_to!(ResubmitArgs for [
    DownloadJobParams,
    ConversionJobParams,
    ImagingJobParams,
    ImageFromJobParams,
] {
    copy: [],
    nullable: [allow_resubmit],
    clone: [],
});

impl_apply_to!(PreprocessingArgs for [ConversionJobParams, ImagingJobParams] {
    copy: [
        apply_di_cal,
        centre,
        custom_centre_ra,
        custom_centre_dec,
        no_apply_amps,
    ],
    nullable: [
        avg_freq_res,
        avg_time_res,
        flag_edge_width,
        no_digital_gains,
        no_flag_dc,
        no_geometric_delay,
        no_passband_gains,
        no_cable_delay,
        no_rfi,
    ],
    clone: [],
});

impl_apply_to!(WscleanArgs for [ImagingJobParams, ImageFromJobParams] {
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
        output_mode,
        pixel_scale,
        polarization,
        robust,
        uv_max,
        uv_min,
        weighting,
        wstack_nwlayers,
    ],
    nullable: [abs_threshold],
    clone: [image_size],
});

/// Arguments for a download job (`submit-download`): the schema's
/// `DownloadJobParams`. The body does not say what kind of data to download:
/// the MWA ASVO finds it from the observation, and refuses the voltage
/// options for an observation that has no voltages. The deprecated download
/// commands make one of these from their own arguments
/// ([`DownloadJobArgs::to_vis_download`], [`DownloadJobArgs::to_meta_download`],
/// [`VoltageJobArgs::to_download`]).
#[derive(clap::Args, Debug, Clone)]
pub struct SubmitDownloadArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<DownloadJobParams>,

    #[arg(
        help = download_help::ONLY_METADATA,
        long,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        action = ArgAction::Set,
    )]
    pub only_metadata: Option<bool>,

    #[arg(help = help_with_range(download_help::OFFSET, validate::VOLTAGE_OFFSET), short, long, value_parser = parse_i64_bounds(validate::VOLTAGE_OFFSET))]
    pub offset: Option<i64>,

    #[arg(help = help_with_range(download_help::DURATION, validate::VOLTAGE_DURATION), short = 'u', long, value_parser = parse_i64_bounds(validate::VOLTAGE_DURATION))]
    pub duration: Option<i64>,

    #[arg(help = help_with_range(download_help::FROM_CHANNEL, RECEIVER_CHANNEL_RANGE), long)]
    pub from_channel: Option<u8>,

    #[arg(help = help_with_range(download_help::TO_CHANNEL, RECEIVER_CHANNEL_RANGE), long)]
    pub to_channel: Option<u8>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<DownloadJobParams>,
}

impl SubmitDownloadArgs {
    /// The request body for a download job for a single Obs ID. The CLI has a
    /// value for every option (its defaults are the schema's), so each field
    /// that the options cover is set. The `to_params` of the other job
    /// arguments do the same.
    pub fn to_params(&self, obs_id: ObsId) -> DownloadJobParams {
        let mut params = DownloadJobParams {
            only_metadata: self.only_metadata,
            offset: self.offset,
            duration: self.duration,
            from_channel: self.from_channel,
            to_channel: self.to_channel,
            ..download_params(obs_id)
        };
        self.delivery_args.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }

    /// The arguments with the delivery and `resubmit` given, `only_metadata`
    /// as given, and no voltage option.
    fn without_voltage(
        delivery_args: DeliveryArgs<DownloadJobParams>,
        only_metadata: Option<bool>,
        resubmit: ResubmitArgs<DownloadJobParams>,
    ) -> Self {
        Self {
            delivery_args,
            only_metadata,
            offset: None,
            duration: None,
            from_channel: None,
            to_channel: None,
            resubmit,
        }
    }
}

/// The arguments of the deprecated `submit-vis`, `submit-meta` and
/// `submit-bf`, which are those of `submit-download` without
/// `--only-metadata` and the voltage options.
#[derive(clap::Args, Debug, Clone)]
pub struct DownloadJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<DownloadJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<DownloadJobParams>,
}

impl DownloadJobArgs {
    /// The `submit-download` arguments of `submit-vis` and `submit-bf`:
    /// `only_metadata` at the schema's default.
    pub fn to_vis_download(&self) -> SubmitDownloadArgs {
        SubmitDownloadArgs::without_voltage(
            self.delivery_args.clone(),
            download_defaults().only_metadata,
            self.resubmit.clone(),
        )
    }

    /// The `submit-download` arguments of `submit-meta`: `only_metadata`
    /// true.
    pub fn to_meta_download(&self) -> SubmitDownloadArgs {
        SubmitDownloadArgs::without_voltage(
            self.delivery_args.clone(),
            Some(true),
            self.resubmit.clone(),
        )
    }
}

/// Arguments for a preprocessing/conversion job.
#[derive(clap::Args, Debug, Clone)]
pub struct ConversionJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ConversionJobParams>,

    #[arg(help = cmd_help::conversion_job_params::OUTPUT, short = 'o', long, default_value_t = conversion_defaults().output, value_parser = SchemaEnumParser::<OutputFormat>::new())]
    pub output: OutputFormat,

    #[command(flatten)]
    pub preprocessing: PreprocessingArgs<ConversionJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ConversionJobParams>,
}

impl ConversionJobArgs {
    /// The request body for a conversion job for a single Obs ID.
    pub fn to_params(&self, obs_id: ObsId) -> ConversionJobParams {
        let mut params = conversion_params(obs_id);
        self.delivery_args.apply_to(&mut params);
        params.output = self.output;
        self.preprocessing.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for an imaging job that starts from raw visibilities.
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingJobArgs {
    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ImagingJobParams>,

    #[command(flatten)]
    pub preprocessing: PreprocessingArgs<ImagingJobParams>,

    #[command(flatten)]
    pub wsclean: WscleanArgs<ImagingJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ImagingJobParams>,
}

impl ImagingJobArgs {
    /// The request body for an imaging job for a single Obs ID.
    pub fn to_params(&self, obs_id: ObsId) -> ImagingJobParams {
        let mut params = imaging_params(obs_id, self.wsclean.output_mode);
        self.delivery_args.apply_to(&mut params);
        self.preprocessing.apply_to(&mut params);
        self.wsclean.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// Arguments for an imaging job that starts from an existing conversion job.
#[derive(clap::Args, Debug, Clone)]
pub struct ImagingFromJobArgs {
    #[arg(help = cmd_help::image_from_job_params::SOURCE_JOB_ID, long)]
    pub source_job_id: NonZeroU64,

    #[command(flatten)]
    pub delivery_args: DeliveryArgs<ImageFromJobParams>,

    #[command(flatten)]
    pub wsclean: WscleanArgs<ImageFromJobParams>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<ImageFromJobParams>,
}

impl ImagingFromJobArgs {
    /// The request body for an image of the conversion job
    /// `--source-job-id`. The MWA ASVO takes the Obs ID from the source job.
    pub fn to_params(&self) -> ImageFromJobParams {
        let mut params = image_from_job_params(self.source_job_id, self.wsclean.output_mode);
        self.delivery_args.apply_to(&mut params);
        self.wsclean.apply_to(&mut params);
        self.resubmit.apply_to(&mut params);
        params
    }
}

/// The arguments of the deprecated `submit-volt`. A voltage job has no
/// delivery options: the body has the schema's delivery defaults, and the
/// MWA ASVO decides where a voltage job is delivered.
#[derive(clap::Args, Debug, Clone)]
pub struct VoltageJobArgs {
    #[arg(help = help_with_range(download_help::OFFSET, validate::VOLTAGE_OFFSET), short, long, value_parser = parse_i64_bounds(validate::VOLTAGE_OFFSET))]
    pub offset: i64,

    #[arg(help = help_with_range(download_help::DURATION, validate::VOLTAGE_DURATION), short = 'u', long, value_parser = parse_i64_bounds(validate::VOLTAGE_DURATION))]
    pub duration: i64,

    #[arg(help = help_with_range(download_help::FROM_CHANNEL, RECEIVER_CHANNEL_RANGE), long)]
    pub from_channel: Option<u8>,

    #[arg(help = help_with_range(download_help::TO_CHANNEL, RECEIVER_CHANNEL_RANGE), long)]
    pub to_channel: Option<u8>,

    #[command(flatten)]
    pub resubmit: ResubmitArgs<DownloadJobParams>,
}

impl VoltageJobArgs {
    /// The `submit-download` arguments of `submit-volt`: the voltage
    /// options, and the schema's defaults for the delivery and
    /// `only_metadata`.
    pub fn to_download(&self) -> SubmitDownloadArgs {
        SubmitDownloadArgs {
            offset: Some(self.offset),
            duration: Some(self.duration),
            from_channel: self.from_channel,
            to_channel: self.to_channel,
            ..SubmitDownloadArgs::without_voltage(
                DeliveryArgs::schema_defaults(),
                download_defaults().only_metadata,
                self.resubmit.clone(),
            )
        }
    }
}
