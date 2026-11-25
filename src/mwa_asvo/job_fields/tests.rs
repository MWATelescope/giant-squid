// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use serde_json::{json, Value};

use super::{job_field_names, job_field_value, parse_job_field, PARAMS_TYPES, TOP_LEVEL_FIELDS};
use crate::mwa_asvo::api::client::typed_job_params;
use crate::mwa_asvo::api::openapi::JobDetailResponse;
use crate::mwa_asvo::MwaAsvoJob;
use crate::test_common::TEST_OBS_ID;

const TEST_OBS_ID_NUMBER: u64 = 1_065_880_128;

/// A job of `job_type` with the `job_params` JSON, a time with a zone, and
/// an error.
fn job(job_type: &str, job_params: Value) -> MwaAsvoJob {
    let mut detail: JobDetailResponse = serde_json::from_value(json!({
        "job_id": 101,
        "obs_id": TEST_OBS_ID_NUMBER,
        "user_id": 4242,
        "job_type": job_type,
        "job_state": "error",
        "job_params": job_params.clone(),
        "created": "2026-09-08T05:41:54Z",
        "started": null,
        "completed": null,
        "modified": null,
        "error_code": 7,
        "error_text": "it failed",
        "first_name": "Test",
        "last_name": "User",
    }))
    .expect("a valid job");
    // As the client reads the params of a listed job.
    detail.job_params = typed_job_params(detail.job_type, Some(job_params));
    MwaAsvoJob::try_from(detail).expect("a usable job")
}

fn conversion_job() -> MwaAsvoJob {
    job(
        "conversion",
        json!({"obs_id": TEST_OBS_ID_NUMBER, "avg_time_res": 0.5, "avg_freq_res": null}),
    )
}

#[test]
fn the_field_names_are_the_top_level_fields_then_the_params_keys_once() {
    let names = job_field_names();

    assert_eq!(names[..TOP_LEVEL_FIELDS.len()], TOP_LEVEL_FIELDS);
    for name in [
        "avg_time_res",
        "flag_edge_width",
        "source_job_id",
        "offset",
        "polarization",
    ] {
        assert!(names.iter().any(|n| n == name), "{name} is a field");
    }
    let mut sorted = names.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "no name is twice");
}

#[test]
fn the_fields_that_the_table_shows_are_not_fields() {
    for name in [
        "job_id",
        "obs_id",
        "job_type",
        "job_state",
        "product",
        "completed",
        "job_params",
    ] {
        assert!(
            !job_field_names().iter().any(|n| n == name),
            "{name} is in the table already"
        );
    }
}

#[test]
fn the_top_level_fields_are_keys_of_the_job_detail_in_the_schema() {
    let schema: Value = serde_json::from_str(super::SCHEMA_JSON).expect("the schema is JSON");
    let keys = &schema["definitions"]["JobDetailResponse"]["properties"];
    for name in TOP_LEVEL_FIELDS {
        assert!(
            keys.get(name).is_some(),
            "{name} is a key of the job detail"
        );
    }
    for definition in PARAMS_TYPES {
        assert!(schema["definitions"][definition]["properties"].is_object());
    }
}

#[test]
fn a_field_name_ignores_case_hyphens_and_underscores() {
    assert_eq!(parse_job_field("Avg-Time_Res"), Ok("avg_time_res"));
    assert_eq!(parse_job_field("ERRORTEXT"), Ok("error_text"));
}

#[test]
fn an_unknown_field_is_an_error_that_lists_the_fields() {
    let err = parse_job_field("avg_time").expect_err("not a field");

    assert!(err.starts_with("`avg_time` is not a job field: expected one of: user_id, "));
    assert!(err.contains(", avg_time_res, "));
}

#[test]
fn a_conversion_job_has_the_values_of_its_params() {
    let job = conversion_job();

    assert_eq!(
        job_field_value(&job, "avg_time_res").as_deref(),
        Some("0.5")
    );
    // Sent as null: the job has the field, with no value.
    assert_eq!(
        job_field_value(&job, "avg_freq_res").as_deref(),
        Some("null")
    );
    // Not sent: the schema's default.
    assert_eq!(
        job_field_value(&job, "flag_edge_width").as_deref(),
        Some("80.0")
    );
    assert_eq!(job_field_value(&job, "delivery").as_deref(), Some("acacia"));
}

#[test]
fn a_field_that_the_job_type_does_not_have_has_no_value() {
    let visibility = job(
        "visibility",
        json!({"obs_id": TEST_OBS_ID_NUMBER, "delivery": "acacia"}),
    );

    assert_eq!(job_field_value(&visibility, "avg_time_res"), None);
    assert_eq!(job_field_value(&visibility, "polarization"), None);
    assert_eq!(
        job_field_value(&visibility, "delivery").as_deref(),
        Some("acacia")
    );
    assert_eq!(
        job_field_value(&visibility, "only_metadata").as_deref(),
        Some("null")
    );
    assert_eq!(job_field_value(&conversion_job(), "offset"), None);
}

#[test]
fn an_imaging_job_has_the_fields_of_its_kind() {
    let from_obs_id = job(
        "imaging",
        json!({"obs_id": TEST_OBS_ID_NUMBER, "output_mode": "fits", "polarization": "iquv"}),
    );
    let from_job = job(
        "imaging",
        json!({"source_job_id": 55, "output_mode": "fits", "image_size": 3072}),
    );

    assert_eq!(
        job_field_value(&from_obs_id, "polarization").as_deref(),
        Some("iquv")
    );
    assert_eq!(job_field_value(&from_obs_id, "source_job_id"), None);
    assert_eq!(
        job_field_value(&from_obs_id, "avg_time_res").as_deref(),
        Some("2.0")
    );
    assert_eq!(
        job_field_value(&from_job, "source_job_id").as_deref(),
        Some("55")
    );
    assert_eq!(
        job_field_value(&from_job, "image_size").as_deref(),
        Some("3072")
    );
    assert_eq!(job_field_value(&from_job, "avg_time_res"), None);
}

#[test]
fn a_top_level_field_is_the_value_of_the_job() {
    let job = conversion_job();

    assert_eq!(job_field_value(&job, "user_id").as_deref(), Some("4242"));
    assert_eq!(
        job_field_value(&job, "error_text").as_deref(),
        Some("it failed")
    );
    assert_eq!(job_field_value(&job, "error_code").as_deref(), Some("7"));
    assert_eq!(
        job_field_value(&job, "created").as_deref(),
        Some("2026-09-08T05:41:54Z")
    );
    assert_eq!(job_field_value(&job, "started").as_deref(), Some("null"));
}

#[test]
fn a_job_that_has_no_params_has_empty_params_fields_only() {
    let cancel = job("cancel", json!({"obs_id": TEST_OBS_ID_NUMBER}));

    assert_eq!(job_field_value(&cancel, "delivery"), None);
    assert_eq!(
        job_field_value(&cancel, "error_text").as_deref(),
        Some("it failed")
    );
}

#[test]
fn a_job_whose_params_the_schema_cannot_read_has_empty_params_fields() {
    // An old job: the Obs ID is a string.
    let old = job(
        "conversion",
        json!({"obs_id": TEST_OBS_ID, "avg_time_res": "0.5"}),
    );

    assert_eq!(job_field_value(&old, "avg_time_res"), None);
    assert_eq!(job_field_value(&old, "user_id").as_deref(), Some("4242"));
}

#[test]
fn a_name_that_is_not_a_field_has_no_value() {
    assert_eq!(job_field_value(&conversion_job(), "nonsense"), None);
    assert_eq!(job_field_value(&conversion_job(), "job_id"), None);
}
