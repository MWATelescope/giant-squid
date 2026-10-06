// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks of job arguments against the limits in the MWA ASVO OpenAPI
//! schema.
//!
//! The generated request types (see [`super::openapi`]) enforce enums and
//! non-zero integers, but not the `minimum` and `maximum` of a number. This
//! module holds those limits once, so that every caller checks the same
//! values: the CLI (which uses them as `clap` value parsers, to fail
//! early), the Python module, and any Rust program that uses the library.
//! The client's submit methods for conversion, imaging, image-from-job and
//! voltage jobs check their request body with the `validate_*_params`
//! function for that body, so an out-of-range value is never sent. The
//! download and beamformer bodies have no numeric limits.
//!
//! Each limit is a hand-written constant, not read from the schema at run
//! time. A unit test compares every constant with the schema file in this
//! directory, so when `tools/generate_openapi.sh` brings in a schema with
//! different limits, the tests fail until the constant is updated. A limit
//! in the schema is never overridden here; a wrong limit is fixed in the
//! API.

use std::num::NonZeroU64;

use super::error::AsvoApiError;
use super::openapi::{
    ConversionJobParams, ImageSizes, ImagingJobFlow1Params, ImagingJobFlow2Params, VoltageJobParams,
};

#[cfg(test)]
mod tests;

/// An inclusive range of numbers. A `None` end has no limit.
///
/// Integer fields are compared as `f64`. That is exact for every limit in
/// the schema, which are all far below 2^53.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// The smallest allowed value, or `None` for no lower limit.
    pub min: Option<f64>,
    /// The largest allowed value, or `None` for no upper limit.
    pub max: Option<f64>,
}

impl Bounds {
    /// A range with both ends.
    pub const fn between(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    /// A range with a lower end only.
    pub const fn at_least(min: f64) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    /// A range with an upper end only.
    pub const fn at_most(max: f64) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    /// Whether `value` is in the range. `NaN` never is.
    pub fn contains(&self, value: f64) -> bool {
        !value.is_nan()
            && self.min.is_none_or(|min| value >= min)
            && self.max.is_none_or(|max| value <= max)
    }

    /// What is wrong with `value`, in words that follow the argument's
    /// name. For example: `must be between 0.1 and 1 (got 1.5)`.
    pub fn describe(&self, value: f64) -> String {
        match (self.min, self.max) {
            (Some(min), Some(max)) => format!("must be between {min} and {max} (got {value})"),
            (Some(min), None) => format!("must be at least {min} (got {value})"),
            (None, Some(max)) => format!("must be at most {max} (got {value})"),
            (None, None) => format!("has no limits (got {value})"),
        }
    }

    /// Check that `value`, the value of the argument `name`, is in the
    /// range.
    ///
    /// # Errors
    ///
    /// [`AsvoApiError::InvalidParameter`] if it is not.
    pub fn check(&self, name: &'static str, value: f64) -> Result<(), AsvoApiError> {
        if self.contains(value) {
            Ok(())
        } else {
            Err(AsvoApiError::InvalidParameter {
                name,
                message: self.describe(value),
            })
        }
    }
}

// The limits below are those of the MWA ASVO OpenAPI schema. One constant
// serves a field in every request body that has it (for example
// `avg_freq_res` in the conversion and imaging bodies), because the schema
// gives that field the same limits in each. The unit tests check this for
// every body.

/// `days` of a job listing (`JobsByUserRequest`): the past number of days to
/// list. The schema gives it a minimum of 1 and a maximum of 30. (Before
/// schema v1.12 the minimum was an `exclusiveMinimum` of 1, which the API
/// developer has fixed.)
pub const DAYS: Bounds = Bounds::between(1.0, 30.0);
/// `abs_threshold`: the absolute cleaning threshold (Jy).
pub const ABS_THRESHOLD: Bounds = Bounds::between(0.0, 10.0);
/// `auto_mask`: the WSClean -auto-mask value.
pub const AUTO_MASK: Bounds = Bounds::between(2.0, 512.0);
/// `auto_threshold`: the WSClean -auto-threshold value.
pub const AUTO_THRESHOLD: Bounds = Bounds::between(0.1, 5.0);
/// `avg_freq_res`: the frequency resolution to average to (kHz).
pub const AVG_FREQ_RES: Bounds = Bounds::between(0.0, 1280.0);
/// `avg_time_res`: the time resolution to average to (s).
pub const AVG_TIME_RES: Bounds = Bounds::at_least(0.0);
/// `clean_iterations`: the WSClean -niter value. The schema sets only the
/// upper limit.
pub const CLEAN_ITERATIONS: Bounds = Bounds::at_most(1_000_000.0);
/// `clean_threshold`: the WSClean cleaning threshold (Jy).
pub const CLEAN_THRESHOLD: Bounds = Bounds::between(0.0, 10.0);
/// `custom_centre_dec`: the custom phase centre's declination (degrees).
pub const CUSTOM_CENTRE_DEC: Bounds = Bounds::between(-90.0, 90.0);
/// `custom_centre_ra`: the custom phase centre's right ascension (degrees).
pub const CUSTOM_CENTRE_RA: Bounds = Bounds::between(0.0, 359.999999);
/// `flag_edge_width`: the width of the frequency edge flagging (kHz).
pub const FLAG_EDGE_WIDTH: Bounds = Bounds::between(0.0, 640.0);
/// `mgain`: the WSClean -mgain value.
pub const MGAIN: Bounds = Bounds::between(0.1, 1.0);
/// `nmiter`: the WSClean -nmiter value.
pub const NMITER: Bounds = Bounds::between(1.0, 500.0);
/// `nwlayers`: the number of w-projection layers (deprecated in the API).
pub const NWLAYERS: Bounds = Bounds::between(32.0, 512.0);
/// `pixel_scale`: the pixel scale (arcsec per pixel).
pub const PIXEL_SCALE: Bounds = Bounds::between(10.0, 120.0);
/// `robust`: the WSClean -robust value.
pub const ROBUST: Bounds = Bounds::between(-2.0, 2.0);
/// `uvw_max`: the maximum uv distance to image (wavelengths).
pub const UVW_MAX: Bounds = Bounds::between(1.0, 5000.0);
/// `uvw_min`: the minimum uv distance to image (wavelengths).
pub const UVW_MIN: Bounds = Bounds::at_most(100.0);
/// `wstack_nwlayers`: the number of w-stacking layers. (Both imaging bodies
/// since schema v1.11; before, only the imaging (flow 1) body.)
pub const WSTACK_NWLAYERS: Bounds = Bounds::between(32.0, 512.0);
/// `offset` of a voltage job: seconds from the start of the observation.
pub const VOLTAGE_OFFSET: Bounds = Bounds::between(0.0, 5400.0);

/// The image sizes (pixels) that the MWA ASVO accepts.
pub const IMAGE_SIZES: [i64; 6] = [512, 1024, 2048, 3072, 4096, 8192];

/// Check each `(name, bounds, value)` whose value is present.
fn check_all(checks: &[(&'static str, Bounds, Option<f64>)]) -> Result<(), AsvoApiError> {
    for (name, bounds, value) in checks {
        if let Some(value) = value {
            bounds.check(name, *value)?;
        }
    }
    Ok(())
}

/// The checks for the fields that both imaging request bodies have.
macro_rules! shared_imaging_checks {
    ($params:expr) => {
        [
            ("abs_threshold", ABS_THRESHOLD, $params.abs_threshold),
            ("auto_mask", AUTO_MASK, Some($params.auto_mask as f64)),
            (
                "auto_threshold",
                AUTO_THRESHOLD,
                Some($params.auto_threshold),
            ),
            (
                "clean_iterations",
                CLEAN_ITERATIONS,
                Some($params.clean_iterations as f64),
            ),
            ("clean_threshold", CLEAN_THRESHOLD, $params.clean_threshold),
            ("mgain", MGAIN, Some($params.mgain)),
            ("nmiter", NMITER, Some($params.nmiter.get() as f64)),
            ("nwlayers", NWLAYERS, $params.nwlayers.map(|n| n as f64)),
            ("pixel_scale", PIXEL_SCALE, Some($params.pixel_scale)),
            ("robust", ROBUST, Some($params.robust)),
            ("uvw_max", UVW_MAX, $params.uvw_max),
            ("uvw_min", UVW_MIN, Some($params.uvw_min)),
            (
                "wstack_nwlayers",
                WSTACK_NWLAYERS,
                $params.wstack_nwlayers.map(|n| n as f64),
            ),
        ]
    };
}

/// Check the numbers of a conversion request body against the schema's
/// limits.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`], for the first argument that is out
/// of range.
pub fn validate_conversion_params(params: &ConversionJobParams) -> Result<(), AsvoApiError> {
    check_all(&[
        ("avg_freq_res", AVG_FREQ_RES, Some(params.avg_freq_res)),
        ("avg_time_res", AVG_TIME_RES, Some(params.avg_time_res)),
        (
            "flag_edge_width",
            FLAG_EDGE_WIDTH,
            Some(params.flag_edge_width),
        ),
        (
            "custom_centre_ra",
            CUSTOM_CENTRE_RA,
            params.custom_centre_ra,
        ),
        (
            "custom_centre_dec",
            CUSTOM_CENTRE_DEC,
            params.custom_centre_dec,
        ),
    ])
}

/// Check the numbers of a voltage request body against the schema's
/// limits. `duration`, `from_channel` and `to_channel` need no check: the
/// schema's limits for them are the limits of their types (`u64`, `u8`).
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `offset` is out of range.
pub fn validate_voltage_params(params: &VoltageJobParams) -> Result<(), AsvoApiError> {
    VOLTAGE_OFFSET.check("offset", params.offset as f64)
}

/// Check the numbers of an imaging (flow 1) request body against the
/// schema's limits.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`], for the first argument that is out
/// of range.
pub fn validate_imaging_params(params: &ImagingJobFlow1Params) -> Result<(), AsvoApiError> {
    check_all(&shared_imaging_checks!(params))?;
    check_all(&[
        ("avg_freq_res", AVG_FREQ_RES, Some(params.avg_freq_res)),
        ("avg_time_res", AVG_TIME_RES, Some(params.avg_time_res)),
        (
            "custom_centre_dec",
            CUSTOM_CENTRE_DEC,
            params.custom_centre_dec,
        ),
        (
            "custom_centre_ra",
            CUSTOM_CENTRE_RA,
            params.custom_centre_ra,
        ),
        (
            "flag_edge_width",
            FLAG_EDGE_WIDTH,
            Some(params.flag_edge_width),
        ),
    ])
}

/// Check the numbers of an image-from-job (flow 2) request body against the
/// schema's limits.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`], for the first argument that is out
/// of range.
pub fn validate_image_from_job_params(params: &ImagingJobFlow2Params) -> Result<(), AsvoApiError> {
    check_all(&shared_imaging_checks!(params))
}

/// An image size, checked against the sizes that the MWA ASVO accepts
/// ([`IMAGE_SIZES`]).
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `value` is not one of them.
pub fn image_size(value: i64) -> Result<ImageSizes, AsvoApiError> {
    ImageSizes::try_from(value).map_err(|_| AsvoApiError::InvalidParameter {
        name: "image_size",
        message: format!(
            "must be one of {} (got {value})",
            IMAGE_SIZES.map(|s| s.to_string()).join(", ")
        ),
    })
}

/// A `nmiter` value, checked against [`NMITER`]. The type enforces the
/// lower limit; this makes the message the same as for any other limit.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `value` is outside [`NMITER`].
pub fn nmiter(value: u64) -> Result<NonZeroU64, AsvoApiError> {
    NMITER.check("nmiter", value as f64)?;
    // NMITER's lower limit is 1, so a value that passed is not zero.
    Ok(NonZeroU64::new(value).expect("NMITER excludes zero"))
}

/// A `days` value for a job listing, checked against [`DAYS`]. The type of
/// the request field enforces the lower limit; this makes the message the
/// same as for any other limit.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `value` is outside [`DAYS`].
pub fn days(value: i64) -> Result<NonZeroU64, AsvoApiError> {
    DAYS.check("days", value as f64)?;
    // DAYS' lower limit is 1, so a value that passed is positive.
    Ok(NonZeroU64::new(value as u64).expect("DAYS excludes zero"))
}

/// A `days` value of the schema's type, checked against the upper limit of
/// [`DAYS`] (the type enforces the lower one).
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `value` is above [`DAYS`].
pub fn check_days(value: NonZeroU64) -> Result<NonZeroU64, AsvoApiError> {
    DAYS.check("days", value.get() as f64)?;
    Ok(value)
}

/// A Job ID for `source_job_id`, which the schema requires to be at least
/// 1.
///
/// # Errors
///
/// [`AsvoApiError::InvalidParameter`] if `value` is zero.
pub fn source_job_id(value: u64) -> Result<NonZeroU64, AsvoApiError> {
    NonZeroU64::new(value).ok_or_else(|| AsvoApiError::InvalidParameter {
        name: "source_job_id",
        message: "must be at least 1 (got 0)".to_string(),
    })
}
