// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super`] ASVO data types.

use super::*;
use crate::mwa_asvo::api::openapi::{JobProduct, Type as FileType};
use crate::test_config::job_type;

/// An obsid used by these tests.
const OBS_ID: u64 = 1065880128;
/// Job IDs used by these tests.
const JOB_ID_A: AsvoJobId = crate::test_config::job_id(101);
const JOB_ID_B: AsvoJobId = crate::test_config::job_id(102);
const JOB_ID_C: AsvoJobId = crate::test_config::job_id(103);
/// Another obsid, for the filter tests.
const OTHER_OBS_ID: u64 = 1090008640;

/// The user ID of the test jobs.
const TEST_USER_ID: i64 = 4242;

/// When the test jobs were created.
fn test_created() -> jiff::Timestamp {
    "2026-09-08T05:41:54Z"
        .parse::<jiff::Timestamp>()
        .expect("a valid time")
}

/// The schema, for the checks of the job type names.
const SCHEMA: &str = include_str!("../api/openapi-schema.json");

/// The job type names are those that the schema gives the codes, in the
/// description of `JobDetailResponse.job_type` ("0=conversion, 1=visibility,
/// ..."), and every code of the schema's `JobType` has one.
#[test]
fn the_job_type_names_are_the_schema_names() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    let definitions = &schema["definitions"];
    let description = definitions["JobDetailResponse"]["properties"]["job_type"]["description"]
        .as_str()
        .expect("job_type has a description");
    let codes: Vec<i64> = definitions["JobType"]["enum"]
        .as_array()
        .expect("JobType is an enum of codes")
        .iter()
        .map(|code| code.as_i64().expect("a code is an integer"))
        .collect();

    let names = JobType::names();
    assert_eq!(names.len(), codes.len());
    for code in codes {
        let job_type = JobType::try_from(code).expect("a schema code is a JobType");
        let in_schema = format!("{code}={}", job_type.name());
        assert!(
            description.contains(&in_schema),
            "{in_schema} is not in the schema's description: {description}"
        );
    }
}

/// The names are listed in the order of their codes.
#[test]
fn the_job_type_names_are_listed_in_order() {
    assert_eq!(
        JobType::names(),
        [
            "conversion",
            "visibility",
            "metadata",
            "voltage",
            "cancel",
            "beamformer",
            "imaging",
        ]
    );
}

/// Each name parses to its type, in any case and spelling of the separators.
#[test]
fn a_job_type_is_parsed_from_its_name() {
    for name in JobType::names() {
        let job_type = JobType::parse_name(&name).expect("a listed name should parse");
        assert_eq!(job_type.name(), name);
        assert_eq!(job_type.to_string(), name);
    }
    assert_eq!(
        JobType::parse_name("VISIBILITY").ok(),
        JobType::parse_name("visibility").ok()
    );
}

/// Text that is not a job type is an error. The names before 3.0.0 (for
/// example `download_visibilities`) are not job types of the schema, and
/// neither is `unknown`.
#[test]
fn text_that_is_not_a_job_type_is_an_error() {
    for text in [
        "",
        "convertion",
        "unknown",
        "download_visibilities",
        "cancel_job",
    ] {
        let err = JobType::parse_name(text).expect_err("this is not a job type");
        assert!(
            matches!(&err, AsvoError::InvalidJobType { str } if str == text),
            "{text}: {err:?}"
        );
    }
}

/// The job state names are the schema's values, in the schema's order.
#[test]
fn the_job_state_names_are_the_schema_values_in_order() {
    assert_eq!(
        JobState::names(),
        [
            "preparing",
            "queued",
            "waitcal",
            "staging",
            "staged",
            "downloading",
            "preprocessing",
            "imaging",
            "delivering",
            "completed",
            "error",
            "cancelled",
        ]
    );
}

/// Each name parses to the state whose schema value it is.
#[test]
fn every_job_state_name_parses_to_its_state() {
    for name in JobState::names() {
        let state = JobState::parse_name(&name).expect("a listed name should parse");
        assert_eq!(state.to_string(), name);
    }
}

/// A job state is parsed from its name in any case and spelling of the
/// separators.
#[test]
fn a_job_state_is_parsed_from_its_name() {
    let cases = [
        ("queued", JobState::Queued),
        ("WAIT-CAL", JobState::Waitcal),
        ("wait_cal", JobState::Waitcal),
        ("Completed", JobState::Completed),
        ("error", JobState::Error),
        ("CANCELLED", JobState::Cancelled),
    ];
    for (name, expected) in cases {
        assert_eq!(JobState::parse_name(name).ok(), Some(expected), "{name}");
    }
}

/// Text that is not a job state is an error with the text in it. `ready`
/// and `expired` were names before 3.0.0, and are not job states of the
/// schema.
#[test]
fn text_that_is_not_a_job_state_is_an_error() {
    for text in ["", "bogus", "retrieving", "ready", "expired", "completedd"] {
        let err = JobState::parse_name(text).expect_err("this is not a job state");
        assert!(
            matches!(&err, AsvoError::InvalidJobState { str } if str == text),
            "{text}: {err:?}"
        );
    }
}

/// A job with the given ID and state.
fn job(job_id: AsvoJobId, state: JobState) -> AsvoJob {
    crate::test_config::asvo_job(
        ObsId::validate(OBS_ID).expect("the test obsid should be valid"),
        JobDetailResponse {
            id: i64::try_from(job_id.get()).expect("a test job ID fits in i64"),
            job_type: Some(job_type("visibility")),
            job_state: state,
            product: None,
            created: test_created(),
            started: None,
            completed: None,
            modified: None,
            error_code: None,
            error_text: None,
            user_id: TEST_USER_ID,
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            job_params: serde_json::Map::new(),
        },
    )
}

#[test]
fn all_ready_is_true_when_every_job_is_ready() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Completed),
    ]);
    assert!(jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_is_false_while_a_job_is_in_progress() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Queued),
    ]);
    assert!(!jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_ignores_jobs_that_were_not_asked_about() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Cancelled),
    ]);
    assert!(jobs.all_ready(&[JOB_ID_A]).expect("no job has failed"));
}

#[test]
fn all_ready_reports_an_unknown_job() {
    let jobs = AsvoJobVec(vec![job(JOB_ID_A, JobState::Completed)]);
    let err = jobs.all_ready(&[JOB_ID_B]).expect_err("expected an error");
    assert!(
        matches!(err, AsvoError::NoAsvoJob(job_id) if job_id == JOB_ID_B),
        "got {err:?}"
    );
}

#[test]
fn all_ready_reports_a_failed_job_with_its_error() {
    let mut failed = job(JOB_ID_A, JobState::Error);
    failed.detail_mut().error_text = Some("the conversion failed".to_string());
    let jobs = AsvoJobVec(vec![failed]);
    match jobs.all_ready(&[JOB_ID_A]) {
        Err(AsvoError::JobFailed {
            job_id,
            obs_id,
            error,
            error_code,
        }) => {
            assert_eq!(job_id, JOB_ID_A);
            assert_eq!(u64::from(obs_id), OBS_ID);
            assert_eq!(error, "the conversion failed");
            assert_eq!(error_code, None);
        }
        other => panic!("expected JobFailed, got {other:?}"),
    }
}

/// A failed job's error code is in the error, and in its message as
/// `(code N)`; with no code the message has no such text.
#[test]
fn a_failed_jobs_error_code_is_in_the_error_message() {
    let mut failed = job(JOB_ID_A, JobState::Error);
    failed.detail_mut().error_text = Some("the conversion failed".to_string());
    failed.detail_mut().error_code = Some(7);
    let jobs = AsvoJobVec(vec![failed.clone()]);

    let err = jobs.all_ready(&[JOB_ID_A]).expect_err("the job has failed");
    assert!(
        matches!(
            &err,
            AsvoError::JobFailed {
                error_code: Some(7),
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(
        err.to_string(),
        format!("MWA ASVO Job ID {JOB_ID_A} (Obs ID: {OBS_ID}) has an error (code 7): the conversion failed")
    );

    failed.detail_mut().error_code = None;
    let err = AsvoJobVec(vec![failed])
        .all_ready(&[JOB_ID_A])
        .expect_err("the job has failed");
    assert_eq!(
        err.to_string(),
        format!(
            "MWA ASVO Job ID {JOB_ID_A} (Obs ID: {OBS_ID}) has an error: the conversion failed"
        )
    );
}

#[test]
fn all_ready_reports_a_cancelled_job() {
    let cancelled = AsvoJobVec(vec![job(JOB_ID_A, JobState::Cancelled)]);
    assert!(matches!(
        cancelled.all_ready(&[JOB_ID_A]),
        Err(AsvoError::JobCancelled(JOB_ID_A))
    ));
}

#[test]
fn all_ready_reports_the_first_failure_in_the_order_asked() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, JobState::Error),
        job(JOB_ID_B, JobState::Cancelled),
    ]);
    assert!(matches!(
        jobs.all_ready(&[JOB_ID_B, JOB_ID_A]),
        Err(AsvoError::JobCancelled(JOB_ID_B))
    ));
}

/// A job with the given ID, obsid, type and state, for the filter tests.
fn job_with(job_id: AsvoJobId, obs_id: u64, job_type: JobType, state: JobState) -> AsvoJob {
    crate::test_config::asvo_job(
        ObsId::validate(obs_id).expect("the test obsid should be valid"),
        JobDetailResponse {
            id: i64::try_from(job_id.get()).expect("a test job ID fits in i64"),
            job_type: Some(job_type),
            job_state: state,
            product: None,
            created: test_created(),
            started: None,
            completed: None,
            modified: None,
            error_code: None,
            error_text: None,
            user_id: TEST_USER_ID,
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            job_params: serde_json::Map::new(),
        },
    )
}

/// Three jobs that differ in job ID, obsid, type and state.
fn mixed_jobs() -> AsvoJobVec {
    AsvoJobVec(vec![
        job_with(
            JOB_ID_A,
            OBS_ID,
            job_type("conversion"),
            JobState::Completed,
        ),
        job_with(
            JOB_ID_B,
            OTHER_OBS_ID,
            job_type("visibility"),
            JobState::Error,
        ),
        job_with(JOB_ID_C, OBS_ID, job_type("metadata"), JobState::Queued),
    ])
}

/// The job IDs in `jobs`, in order.
fn ids(jobs: &AsvoJobVec) -> Vec<AsvoJobId> {
    jobs.0.iter().map(|j| j.job_id).collect()
}

#[test]
fn filter_with_no_criteria_keeps_every_job() {
    let jobs = mixed_jobs().filter(&[], &[], &[], &[]);
    assert_eq!(ids(&jobs), vec![JOB_ID_A, JOB_ID_B, JOB_ID_C]);
}

#[test]
fn filter_by_job_id() {
    let jobs = mixed_jobs().filter(&[JOB_ID_B, JOB_ID_C], &[], &[], &[]);
    assert_eq!(ids(&jobs), vec![JOB_ID_B, JOB_ID_C]);
}

#[test]
fn filter_by_obs_id() {
    let obs_id = ObsId::validate(OBS_ID).expect("the test obsid should be valid");
    let jobs = mixed_jobs().filter(&[], &[obs_id], &[], &[]);
    assert_eq!(ids(&jobs), vec![JOB_ID_A, JOB_ID_C]);
}

#[test]
fn filter_by_job_type() {
    let jobs = mixed_jobs().filter(&[], &[], &[job_type("metadata")], &[]);
    assert_eq!(ids(&jobs), vec![JOB_ID_C]);
}

#[test]
fn filter_by_state_matches_any_error() {
    let jobs = mixed_jobs().filter(&[], &[], &[], &[JobState::Error, JobState::Completed]);
    assert_eq!(ids(&jobs), vec![JOB_ID_A, JOB_ID_B]);
}

#[test]
fn filter_criteria_are_combined() {
    let obs_id = ObsId::validate(OBS_ID).expect("the test obsid should be valid");
    let jobs = mixed_jobs().filter(&[], &[obs_id], &[], &[JobState::Queued]);
    assert_eq!(ids(&jobs), vec![JOB_ID_C]);
}

/// `giant-squid list --json` and `AsvoJobVec::json` give each job as the
/// schema's `JobDetailResponse`, plus `obs_id`. A field with no value (here
/// `started`, `completed` and the others) has no key, as in the schema.
/// `--legacy-json` keeps the old keys; see `cli::legacy_json`.
#[test]
fn the_json_output_keys_are_the_openapi_names() {
    let mut ready = job(JOB_ID_A, JobState::Completed);
    ready.detail_mut().product = Some(JobProduct {
        files: vec![JobFile {
            type_: FileType::Acacia,
            url: Some("https://example.org/f.tar".to_string()),
            path: None,
            size: 1,
            sha1: Some("0".repeat(40)),
            format: None,
        }],
    });

    let json: serde_json::Value =
        serde_json::from_str(&AsvoJobVec(vec![ready]).json().expect("the jobs serialise"))
            .expect("the output is JSON");
    let entry = &json[JOB_ID_A.to_string()];

    let keys: Vec<&str> = entry
        .as_object()
        .expect("each job is an object")
        .keys()
        .map(String::as_str)
        .collect();
    // serde_json sorts the keys of a parsed object, so compare sorted.
    assert_eq!(
        keys,
        [
            "created",
            "first_name",
            "id",
            "job_params",
            "job_state",
            "job_type",
            "last_name",
            "obs_id",
            "product",
            "user_id"
        ]
    );
    let file_keys: Vec<&str> = entry["product"]["files"][0]
        .as_object()
        .expect("each file is an object")
        .keys()
        .map(String::as_str)
        .collect();
    // A `JobFile` leaves out a key that has no value (here `path` and
    // `format`), as the schema does.
    assert_eq!(file_keys, ["sha1", "size", "type", "url"]);
    assert_eq!(entry["product"]["files"][0]["type"], "acacia");
}
