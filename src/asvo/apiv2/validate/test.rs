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

/// The schema this crate's request types were generated from.
const SCHEMA: &str = include_str!("../openapi-schema.json");

/// The two imaging request bodies.
const FLOW1: &str = "ImagingJobFlow1Params";
const FLOW2: &str = "ImagingJobFlow2Params";

/// The limits of the fields both imaging request bodies have.
const SHARED_LIMITS: [(&str, Bounds); 12] = [
    ("abs_threshold", ABS_THRESHOLD),
    ("auto_mask", AUTO_MASK),
    ("auto_threshold", AUTO_THRESHOLD),
    ("clean_iterations", CLEAN_ITERATIONS),
    ("clean_threshold", CLEAN_THRESHOLD),
    ("mgain", MGAIN),
    ("nmiter", NMITER),
    ("nwlayers", NWLAYERS),
    ("pixel_scale", PIXEL_SCALE),
    ("robust", ROBUST),
    ("uvw_max", UVW_MAX),
    ("uvw_min", UVW_MIN),
];

/// The limits of the fields only the imaging (flow 1) request body has.
const FLOW1_ONLY_LIMITS: [(&str, Bounds); 5] = [
    ("avg_freq_res", AVG_FREQ_RES),
    ("avg_time_res", AVG_TIME_RES),
    ("custom_centre_dec", CUSTOM_CENTRE_DEC),
    ("custom_centre_ra", CUSTOM_CENTRE_RA),
    ("flag_edge_width", FLAG_EDGE_WIDTH),
];

/// The fields whose values are integers in the request body.
const INTEGER_FIELDS: [&str; 4] = ["auto_mask", "clean_iterations", "nmiter", "nwlayers"];

/// Limits this module has that the schema does not state, as
/// `(schema type, field, "minimum" or "maximum")`. See
/// [`CLEAN_ITERATIONS`].
const LIMITS_NOT_IN_SCHEMA: [(&str, &str, &str); 2] = [
    (FLOW1, "clean_iterations", "minimum"),
    (FLOW2, "clean_iterations", "minimum"),
];

fn flow1_defaults() -> ImagingJobFlow1Params {
    ImagingJobFlow1Params::builder()
        .obs_id(1_065_880_128_i64)
        .try_into()
        .expect("defaults build")
}

fn flow2_defaults() -> ImagingJobFlow2Params {
    ImagingJobFlow2Params::builder()
        .obs_id(1_065_880_128_i64)
        .source_job_id(NonZeroU64::new(1).unwrap())
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
fn invalid_name(result: Result<(), AsvoApiError>) -> &'static str {
    match result {
        Err(AsvoApiError::InvalidParameter { name, .. }) => name,
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
    assert_eq!(UVW_MIN.describe(101.0), "must be at most 100 (got 101)");
}

#[test]
fn check_names_the_argument() {
    let err = MGAIN.check("mgain", 1.5).expect_err("1.5 is too big");

    assert!(matches!(
        err,
        AsvoApiError::InvalidParameter { name: "mgain", .. }
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
    validate_imaging_params(&flow1_defaults()).expect("flow 1 defaults are valid");
    validate_image_from_job_params(&flow2_defaults()).expect("flow 2 defaults are valid");
}

#[test]
fn every_limit_is_enforced_on_both_ends_for_the_imaging_body() {
    for (field, bounds) in SHARED_LIMITS.iter().chain(FLOW1_ONLY_LIMITS.iter()) {
        if let Some(max) = bounds.max {
            let params = with_field(&flow1_defaults(), field, field_value(field, max));
            validate_imaging_params(&params)
                .unwrap_or_else(|e| panic!("{field}={max} should be the largest valid value: {e}"));
            let params = with_field(&flow1_defaults(), field, field_value(field, max + 1.0));
            assert_eq!(
                invalid_name(validate_imaging_params(&params)),
                *field,
                "{field} above its maximum"
            );
        }
        // A zero nmiter cannot be built: the type refuses it.
        if let Some(min) = bounds.min.filter(|_| *field != "nmiter") {
            let params = with_field(&flow1_defaults(), field, field_value(field, min));
            validate_imaging_params(&params).unwrap_or_else(|e| {
                panic!("{field}={min} should be the smallest valid value: {e}")
            });
            let params = with_field(&flow1_defaults(), field, field_value(field, min - 1.0));
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
            let params = with_field(&flow2_defaults(), field, field_value(field, max + 1.0));
            assert_eq!(
                invalid_name(validate_image_from_job_params(&params)),
                *field,
                "{field} above its maximum"
            );
        }
        if let Some(min) = bounds.min.filter(|_| *field != "nmiter") {
            let params = with_field(&flow2_defaults(), field, field_value(field, min - 1.0));
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
    let params = flow1_defaults();

    assert!(params.uvw_max.is_none() && params.nwlayers.is_none());
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
        AsvoApiError::InvalidParameter { name, message } => {
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
        if LIMITS_NOT_IN_SCHEMA.contains(&(schema_type, field, key)) {
            assert!(
                theirs.is_none() && ours.is_some(),
                "{schema_type}.{field} {key}: the exemption is out of date (schema {theirs:?}, ours {ours:?})"
            );
        } else {
            assert_eq!(
                ours, theirs,
                "{schema_type}.{field} {key} differs from the schema"
            );
        }
    }
}

#[test]
fn the_limits_are_the_schema_limits() {
    for (field, bounds) in SHARED_LIMITS {
        assert_matches_schema(FLOW1, field, bounds);
        assert_matches_schema(FLOW2, field, bounds);
    }
    for (field, bounds) in FLOW1_ONLY_LIMITS {
        assert_matches_schema(FLOW1, field, bounds);
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
    // Both request bodies use that definition.
    for schema_type in [FLOW1, FLOW2] {
        assert_eq!(
            schema_properties(schema_type)["image_size"]["$ref"],
            "#/definitions/ImageSizes",
            "{schema_type}.image_size"
        );
    }
}

#[test]
fn the_source_job_id_minimum_is_the_schema_minimum() {
    let properties = schema_properties(FLOW2);

    assert_eq!(
        schema_limit(&properties["source_job_id"], "minimum"),
        Some(1.0)
    );
}
