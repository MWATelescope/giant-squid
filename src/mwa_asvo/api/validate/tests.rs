// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the job argument limits.
//!
//! Offline: nothing here contacts a server. The last group compares the
//! limits with the schema file in this directory, which is the check that
//! keeps them from drifting away from the API.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use super::*;
use crate::mwa_asvo::api::openapi::{ImageOutputMode, PhaseCentre};

/// The schema this crate's request types were generated from.
const SCHEMA: &str = include_str!("../openapi-schema.json");

/// The request bodies that have numeric limits, by their schema names.
const CONVERSION: &str = "ConversionJobParams";
const IMAGING: &str = "ImagingJobParams";
const IMAGE_FROM_JOB: &str = "ImageFromJobParams";
/// The download body (visibility, metadata, voltage and beamformer jobs).
const DOWNLOAD: &str = "DownloadJobParams";

/// Every job request body in the schema. The completeness test looks for
/// limits in all of them, so a limit added to any body is noticed.
const JOB_BODIES: [&str; 4] = [DOWNLOAD, CONVERSION, IMAGING, IMAGE_FROM_JOB];

/// The job request bodies that have an `obs_id`. The image-from-job body
/// has none: the MWA ASVO takes it from the source job.
const OBS_ID_BODIES: [&str; 3] = [DOWNLOAD, CONVERSION, IMAGING];

/// The limits of the fields both imaging request bodies have.
const SHARED_LIMITS: [(&str, Bounds); 12] = [
    ("abs_threshold", ABS_THRESHOLD),
    ("auto_mask", AUTO_MASK),
    ("auto_threshold", AUTO_THRESHOLD),
    ("channels_out", CHANNELS_OUT),
    ("clean_iterations", CLEAN_ITERATIONS),
    ("mgain", MGAIN),
    ("nmiter", NMITER),
    ("pixel_scale", PIXEL_SCALE),
    ("robust", ROBUST),
    ("uvw_max", UVW_MAX),
    ("uvw_min", UVW_MIN),
    ("wstack_nwlayers", WSTACK_NWLAYERS),
];

/// The limits of the fields that the imaging and the conversion request
/// bodies have, and the image-from-job body does not.
const CONVERSION_LIMITS: [(&str, Bounds); 5] = [
    ("avg_freq_res", AVG_FREQ_RES),
    ("avg_time_res", AVG_TIME_RES),
    ("custom_centre_dec", CUSTOM_CENTRE_DEC),
    ("custom_centre_ra", CUSTOM_CENTRE_RA),
    ("flag_edge_width", FLAG_EDGE_WIDTH),
];

/// The limits of the download body that this module checks.
const DOWNLOAD_LIMITS: [(&str, Bounds); 2] =
    [("offset", VOLTAGE_OFFSET), ("duration", VOLTAGE_DURATION)];

/// The fields whose lower limit is 1 and whose type (`NonZeroU64`) refuses
/// zero, so a value below the minimum cannot be built to test the check.
const NON_ZERO_FIELDS: [&str; 2] = ["channels_out", "nmiter"];

/// Schema limits that the Rust type enforces, so this module has no check
/// for them: `(body, field, why)`. The completeness test fails for any
/// schema limit that is in neither this list nor a table above.
const TYPE_ENFORCED: [(&str, &str, &str); 6] = [
    (DOWNLOAD, "obs_id", "ObsId::validate has the same minimum"),
    (CONVERSION, "obs_id", "ObsId::validate has the same minimum"),
    (IMAGING, "obs_id", "ObsId::validate has the same minimum"),
    (
        IMAGE_FROM_JOB,
        "source_job_id",
        "NonZeroU64 (see source_job_id())",
    ),
    (DOWNLOAD, "from_channel", "u8 has the range 0 to 255"),
    (DOWNLOAD, "to_channel", "u8 has the range 0 to 255"),
];

/// The fields whose values are integers in the request body.
const INTEGER_FIELDS: [&str; 7] = [
    "auto_mask",
    "channels_out",
    "clean_iterations",
    "nmiter",
    "wstack_nwlayers",
    "offset",
    "duration",
];

/// The smallest Obs ID that [`ObsId::validate`](crate::obs_id::ObsId::validate)
/// accepts. It must be the schema's own minimum.
const SMALLEST_VALID_OBS_ID: f64 = crate::obs_id::ObsId::MIN as f64;

fn imaging_defaults() -> ImagingJobParams {
    ImagingJobParams::builder()
        .obs_id(1_065_880_128_i64)
        .output_mode(ImageOutputMode::Fits)
        .try_into()
        .expect("defaults build")
}

fn conversion_defaults() -> ConversionJobParams {
    ConversionJobParams::builder()
        .obs_id(1_065_880_128_i64)
        .try_into()
        .expect("defaults build")
}

/// A voltage job: a download body with an offset and a duration.
fn voltage_defaults() -> DownloadJobParams {
    DownloadJobParams::builder()
        .obs_id(1_065_880_128_i64)
        .offset(Some(0_i64))
        .duration(Some(8_i64))
        .try_into()
        .expect("defaults build")
}

fn image_from_job_defaults() -> ImageFromJobParams {
    ImageFromJobParams::builder()
        .source_job_id(NonZeroU64::new(1).unwrap())
        .output_mode(ImageOutputMode::Fits)
        .try_into()
        .expect("defaults build")
}

/// `base` with the field `field` set to `value`, through JSON, so that a
/// test can put a value in any field without a setter for each.
fn with_field<T: Serialize + DeserializeOwned>(base: &T, field: &str, value: Value) -> T {
    let mut json = serde_json::to_value(base).expect("params serialise");
    json[field] = value;
    serde_json::from_value(json).expect("params deserialise")
}

/// `number` as the JSON value of `field`.
fn field_value(field: &str, number: f64) -> Value {
    if INTEGER_FIELDS.contains(&field) {
        json!(number as i64)
    } else {
        json!(number)
    }
}

/// The name in an `InvalidParameter` error.
fn invalid_name(result: Result<(), MwaAsvoApiError>) -> &'static str {
    match result {
        Err(MwaAsvoApiError::InvalidParameter { name, .. }) => name,
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

#[test]
fn bounds_include_both_ends() {
    let b = Bounds::between(0.1, 1.0);

    assert!(b.contains(0.1));
    assert!(b.contains(1.0));
    assert!(!b.contains(0.099));
    assert!(!b.contains(1.001));
}

#[test]
fn a_bound_with_one_end_has_no_limit_at_the_other() {
    assert!(Bounds::at_least(0.0).contains(1e300));
    assert!(!Bounds::at_least(0.0).contains(-0.001));
    assert!(Bounds::at_most(100.0).contains(-1e300));
    assert!(!Bounds::at_most(100.0).contains(100.001));
}

#[test]
fn nan_is_never_in_bounds() {
    assert!(!Bounds::between(0.0, 1.0).contains(f64::NAN));
    assert!(!Bounds::at_least(0.0).contains(f64::NAN));
    assert!(!Bounds::at_most(0.0).contains(f64::NAN));
}

#[test]
fn the_message_says_what_the_limit_is() {
    assert_eq!(
        MGAIN.describe(1.5),
        "must be between 0.1 and 1 (got 1.5)".to_string()
    );
    assert_eq!(AVG_TIME_RES.describe(-1.0), "must be at least 0 (got -1)");
    assert_eq!(
        Bounds::at_most(100.0).describe(101.0),
        "must be at most 100 (got 101)"
    );
}

#[test]
fn check_names_the_argument() {
    let err = MGAIN.check("mgain", 1.5).expect_err("1.5 is too big");

    assert!(matches!(
        err,
        MwaAsvoApiError::InvalidParameter { name: "mgain", .. }
    ));
    assert_eq!(
        err.to_string(),
        "Invalid mgain: must be between 0.1 and 1 (got 1.5)"
    );
    assert!(MGAIN.check("mgain", 0.8).is_ok());
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[test]
fn the_schema_defaults_are_within_the_limits() {
    validate_imaging_params(&imaging_defaults()).expect("imaging defaults are valid");
    validate_image_from_job_params(&image_from_job_defaults())
        .expect("image-from-job defaults are valid");
    validate_conversion_params(&conversion_defaults()).expect("conversion defaults are valid");
    validate_download_params(&voltage_defaults()).expect("voltage defaults are valid");
}

/// Check both ends of each of `limits` with `validate`, starting from
/// `defaults`: the end itself is valid, one past it names the field.
fn assert_limits_enforced<T: Serialize + DeserializeOwned>(
    defaults: &T,
    limits: &[(&str, Bounds)],
    validate: fn(&T) -> Result<(), MwaAsvoApiError>,
) {
    for (field, bounds) in limits {
        for (end, past) in [
            (bounds.max, bounds.max.map(|m| m + 1.0)),
            (bounds.min, bounds.min.map(|m| m - 1.0)),
        ] {
            let (Some(end), Some(past)) = (end, past) else {
                continue;
            };
            let params = with_field(defaults, field, field_value(field, end));
            validate(&params).unwrap_or_else(|e| panic!("{field}={end} should be valid: {e}"));
            let params = with_field(defaults, field, field_value(field, past));
            assert_eq!(invalid_name(validate(&params)), *field, "{field}={past}");
        }
    }
}

/// A valid phase centre for the limit tests: the coordinates of a custom
/// centre are only allowed with `centre` `custom`, so the limits of the
/// coordinates can only be tried from a body that has it.
const CUSTOM_CENTRE_TEST_RA: f64 = 10.0;
const CUSTOM_CENTRE_TEST_DEC: f64 = -26.0;

fn conversion_with_custom_centre() -> ConversionJobParams {
    ConversionJobParams {
        centre: PhaseCentre::Custom,
        custom_centre_ra: Some(CUSTOM_CENTRE_TEST_RA),
        custom_centre_dec: Some(CUSTOM_CENTRE_TEST_DEC),
        ..conversion_defaults()
    }
}

fn imaging_with_custom_centre() -> ImagingJobParams {
    ImagingJobParams {
        centre: PhaseCentre::Custom,
        custom_centre_ra: Some(CUSTOM_CENTRE_TEST_RA),
        custom_centre_dec: Some(CUSTOM_CENTRE_TEST_DEC),
        ..imaging_defaults()
    }
}

#[test]
fn every_limit_is_enforced_on_both_ends_for_the_conversion_body() {
    assert_limits_enforced(
        &conversion_with_custom_centre(),
        &CONVERSION_LIMITS,
        validate_conversion_params,
    );
}

#[test]
fn every_limit_is_enforced_on_both_ends_for_the_download_body() {
    assert_limits_enforced(
        &voltage_defaults(),
        &DOWNLOAD_LIMITS,
        validate_download_params,
    );
}

#[test]
fn every_limit_is_enforced_on_both_ends_for_the_imaging_body() {
    for (field, bounds) in SHARED_LIMITS.iter().chain(CONVERSION_LIMITS.iter()) {
        if let Some(max) = bounds.max {
            let params = with_field(
                &imaging_with_custom_centre(),
                field,
                field_value(field, max),
            );
            validate_imaging_params(&params)
                .unwrap_or_else(|e| panic!("{field}={max} should be the largest valid value: {e}"));
            let params = with_field(
                &imaging_with_custom_centre(),
                field,
                field_value(field, max + 1.0),
            );
            assert_eq!(
                invalid_name(validate_imaging_params(&params)),
                *field,
                "{field} above its maximum"
            );
        }
        // A zero nmiter or channels_out cannot be built: the type refuses it.
        if let Some(min) = bounds.min.filter(|_| !NON_ZERO_FIELDS.contains(field)) {
            let params = with_field(
                &imaging_with_custom_centre(),
                field,
                field_value(field, min),
            );
            validate_imaging_params(&params).unwrap_or_else(|e| {
                panic!("{field}={min} should be the smallest valid value: {e}")
            });
            let params = with_field(
                &imaging_with_custom_centre(),
                field,
                field_value(field, min - 1.0),
            );
            assert_eq!(
                invalid_name(validate_imaging_params(&params)),
                *field,
                "{field} below its minimum"
            );
        }
    }
}

#[test]
fn every_limit_is_enforced_on_both_ends_for_the_image_from_job_body() {
    for (field, bounds) in SHARED_LIMITS.iter() {
        if let Some(max) = bounds.max {
            let params = with_field(
                &image_from_job_defaults(),
                field,
                field_value(field, max + 1.0),
            );
            assert_eq!(
                invalid_name(validate_image_from_job_params(&params)),
                *field,
                "{field} above its maximum"
            );
        }
        if let Some(min) = bounds.min.filter(|_| !NON_ZERO_FIELDS.contains(field)) {
            let params = with_field(
                &image_from_job_defaults(),
                field,
                field_value(field, min - 1.0),
            );
            assert_eq!(
                invalid_name(validate_image_from_job_params(&params)),
                *field,
                "{field} below its minimum"
            );
        }
    }
}

#[test]
fn an_absent_optional_field_is_not_checked() {
    let params = imaging_defaults();

    assert!(params.uvw_max.is_none() && params.pixel_scale.is_none());
    validate_imaging_params(&params).expect("absent optional fields are valid");
}

// ---------------------------------------------------------------------------
// Typed values
// ---------------------------------------------------------------------------

#[test]
fn every_accepted_image_size_is_accepted() {
    for size in IMAGE_SIZES {
        assert_eq!(i64::from(image_size(size).expect("valid size")), size);
    }
}

#[test]
fn an_unsupported_image_size_lists_the_supported_ones() {
    let err = image_size(100).expect_err("100 is not a size");

    match err {
        MwaAsvoApiError::InvalidParameter { name, message } => {
            assert_eq!(name, "image_size");
            assert_eq!(
                message,
                "must be one of 512, 1024, 2048, 3072, 4096, 8192 (got 100)"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn nmiter_is_limited_to_the_schema_range() {
    assert_eq!(nmiter(1).unwrap().get(), 1);
    assert_eq!(nmiter(500).unwrap().get(), 500);
    assert_eq!(invalid_name(nmiter(0).map(|_| ())), "nmiter");
    assert_eq!(invalid_name(nmiter(501).map(|_| ())), "nmiter");
}

#[test]
fn channels_out_is_limited_to_the_schema_range() {
    assert_eq!(channels_out(1).unwrap().get(), 1);
    assert_eq!(channels_out(u64::MAX).unwrap().get(), u64::MAX);
    assert_eq!(invalid_name(channels_out(0).map(|_| ())), "channels_out");
}

#[test]
fn days_is_limited_to_the_schema_range() {
    assert_eq!(days(1).unwrap().get(), 1);
    assert_eq!(days(30).unwrap().get(), 30);
    for bad in [i64::MIN, -1, 0, 31, i64::MAX] {
        assert_eq!(invalid_name(days(bad).map(|_| ())), "days", "{bad}");
    }
}

#[test]
fn a_source_job_id_of_zero_is_rejected() {
    assert_eq!(source_job_id(12345).unwrap().get(), 12345);
    assert_eq!(invalid_name(source_job_id(0).map(|_| ())), "source_job_id");
}

// ---------------------------------------------------------------------------
// The limits against the schema
// ---------------------------------------------------------------------------

/// The `properties` object of the schema type `name`.
fn schema_properties(name: &str) -> Value {
    let schema: Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    schema["definitions"][name]["properties"].clone()
}

/// A limit ("minimum" or "maximum") of a schema field. The schema wraps an
/// optional field's constraints in `anyOf`, so this looks there too.
fn schema_limit(field: &Value, key: &str) -> Option<f64> {
    field[key].as_f64().or_else(|| {
        field["anyOf"]
            .as_array()
            .and_then(|alternatives| alternatives.iter().find_map(|a| a[key].as_f64()))
    })
}

/// Assert that `bounds` are the schema's limits for `field` of `schema_type`.
fn assert_matches_schema(schema_type: &str, field: &str, bounds: Bounds) {
    let properties = schema_properties(schema_type);
    let schema_field = &properties[field];
    assert!(
        !schema_field.is_null(),
        "{schema_type} has no field {field} in the schema"
    );

    for (key, ours) in [("minimum", bounds.min), ("maximum", bounds.max)] {
        let theirs = schema_limit(schema_field, key);
        assert_eq!(
            ours, theirs,
            "{schema_type}.{field} {key} differs from the schema"
        );
    }
}

/// Every table of limits, with the bodies it applies to.
fn limit_tables() -> Vec<(&'static str, &'static [(&'static str, Bounds)])> {
    vec![
        (IMAGING, &SHARED_LIMITS),
        (IMAGE_FROM_JOB, &SHARED_LIMITS),
        (IMAGING, &CONVERSION_LIMITS),
        (CONVERSION, &CONVERSION_LIMITS),
        (DOWNLOAD, &DOWNLOAD_LIMITS),
    ]
}

#[test]
fn the_limits_are_the_schema_limits() {
    for (body, table) in limit_tables() {
        for (field, bounds) in table {
            assert_matches_schema(body, field, *bounds);
        }
    }
}

/// `days` is not in a job body but in the job listing's request, so it has
/// its own check against the schema.
#[test]
fn the_days_limit_is_the_schema_limit() {
    assert_matches_schema("JobsByUserRequest", "days", DAYS);
}

#[test]
fn every_schema_limit_on_a_job_body_is_checked() {
    let checked: Vec<(&str, &str)> = limit_tables()
        .into_iter()
        .flat_map(|(body, table)| table.iter().map(move |(field, _)| (body, *field)))
        .collect();

    for body in JOB_BODIES {
        let properties = schema_properties(body);
        let properties = properties
            .as_object()
            .unwrap_or_else(|| panic!("{body} is not in the schema"));
        for (field, schema_field) in properties {
            let limited = ["minimum", "maximum"]
                .iter()
                .any(|key| schema_limit(schema_field, key).is_some());
            if !limited {
                continue;
            }
            let covered = checked.contains(&(body, field.as_str()))
                || TYPE_ENFORCED
                    .iter()
                    .any(|(b, f, _)| *b == body && *f == field.as_str());
            assert!(
                covered,
                "{body}.{field} has a limit in the schema that nothing checks"
            );
        }
    }
}

#[test]
fn the_type_enforced_limits_are_still_the_schema_limits() {
    for body in OBS_ID_BODIES {
        let obs_id_min = schema_limit(&schema_properties(body)["obs_id"], "minimum")
            .unwrap_or_else(|| panic!("{body}.obs_id has no minimum"));
        assert_eq!(
            obs_id_min, SMALLEST_VALID_OBS_ID,
            "{body}.obs_id: the schema minimum is not what ObsId accepts"
        );
    }
    let voltage = schema_properties(DOWNLOAD);
    for channel in ["from_channel", "to_channel"] {
        assert_eq!(schema_limit(&voltage[channel], "minimum"), Some(0.0));
        assert_eq!(
            schema_limit(&voltage[channel], "maximum"),
            Some(f64::from(u8::MAX))
        );
    }
}

#[test]
fn the_image_sizes_are_the_schema_sizes() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    let sizes: Vec<i64> = schema["definitions"]["ImageSizes"]["enum"]
        .as_array()
        .expect("the schema lists the image sizes")
        .iter()
        .map(|v| v.as_i64().expect("sizes are integers"))
        .collect();

    assert_eq!(sizes, IMAGE_SIZES);
    // Both request bodies use that definition (or null).
    for schema_type in [IMAGING, IMAGE_FROM_JOB] {
        let alternatives = schema_properties(schema_type)["image_size"]["anyOf"].clone();
        assert!(
            alternatives
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["$ref"] == "#/definitions/ImageSizes")),
            "{schema_type}.image_size"
        );
    }
}

#[test]
fn the_source_job_id_minimum_is_the_schema_minimum() {
    let properties = schema_properties(IMAGE_FROM_JOB);

    assert_eq!(
        schema_limit(&properties["source_job_id"], "minimum"),
        Some(1.0)
    );
}

/// A plain number that the schema's type does not allow is refused with the
/// library's message when the schema value is made from it.
#[test]
fn a_value_the_schema_does_not_allow_is_refused() {
    let err = image_size(1000).expect_err("1000 is not an image size");
    assert!(
        matches!(
            err,
            MwaAsvoApiError::InvalidParameter {
                name: "image_size",
                ..
            }
        ),
        "{err:?}"
    );

    let err = source_job_id(0).expect_err("0 is not a Job ID");
    assert!(
        matches!(
            err,
            MwaAsvoApiError::InvalidParameter {
                name: "source_job_id",
                ..
            }
        ),
        "{err:?}"
    );
}

/// A value that is not finite is never in bounds: serde_json would send it
/// as `null`.
#[test]
fn an_infinite_value_is_never_in_bounds() {
    assert!(!Bounds::at_least(0.0).contains(f64::INFINITY));
    assert!(!Bounds::at_most(100.0).contains(f64::NEG_INFINITY));
    assert!(!Bounds::between(0.0, 1.0).contains(f64::INFINITY));
}
