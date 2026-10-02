// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super`] ASVO data types.

use super::*;

/// An obsid used by these tests.
const OBS_ID: u64 = 1065880128;
/// Job IDs used by these tests.
const JOB_ID_A: AsvoJobId = 101;
const JOB_ID_B: AsvoJobId = 102;
const JOB_ID_C: AsvoJobId = 103;
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

/// Every job type with a name is parsed from its name in any case and
/// spelling of the separators.
#[test]
fn a_job_type_is_parsed_from_its_name() {
    let cases = [
        ("conversion", AsvoJobType::Conversion),
        ("download_visibilities", AsvoJobType::DownloadVisibilities),
        ("DownloadVisibilities", AsvoJobType::DownloadVisibilities),
        ("download-metadata", AsvoJobType::DownloadMetadata),
        ("download_voltages", AsvoJobType::DownloadVoltage),
        ("download_voltage", AsvoJobType::DownloadVoltage),
        ("DOWNLOAD VOLTAGE", AsvoJobType::DownloadVoltage),
        ("download_beamformer", AsvoJobType::DownloadBeamformer),
        ("cancel_job", AsvoJobType::CancelJob),
        ("imaging", AsvoJobType::Imaging),
    ];
    for (name, expected) in cases {
        assert_eq!(name.parse::<AsvoJobType>().ok(), Some(expected), "{name}");
    }
}

/// Text that is not a job type is an error. "unknown" is one: `Unknown` is
/// for the job types of a newer server, and cannot be asked for.
#[test]
fn text_that_is_not_a_job_type_is_an_error() {
    for text in ["", "convertion", "unknown", "download", "downloadvoltagess"] {
        let err = text
            .parse::<AsvoJobType>()
            .expect_err("this is not a job type");
        assert!(
            matches!(&err, AsvoError::InvalidJobType { str } if str == text),
            "{text}: {err:?}"
        );
    }
}

/// A job with the given ID and state.
fn job(job_id: AsvoJobId, state: AsvoJobState) -> AsvoJob {
    AsvoJob {
        obs_id: ObsId::validate(OBS_ID).expect("the test obsid should be valid"),
        job_id,
        job_type: AsvoJobType::DownloadVisibilities,
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
    }
}

#[test]
fn all_ready_is_true_when_every_job_is_ready() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, AsvoJobState::Ready),
        job(JOB_ID_B, AsvoJobState::Ready),
    ]);
    assert!(jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_is_false_while_a_job_is_in_progress() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, AsvoJobState::Ready),
        job(JOB_ID_B, AsvoJobState::Queued),
    ]);
    assert!(!jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_ignores_jobs_that_were_not_asked_about() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, AsvoJobState::Ready),
        job(JOB_ID_B, AsvoJobState::Cancelled),
    ]);
    assert!(jobs.all_ready(&[JOB_ID_A]).expect("no job has failed"));
}

#[test]
fn all_ready_reports_an_unknown_job() {
    let jobs = AsvoJobVec(vec![job(JOB_ID_A, AsvoJobState::Ready)]);
    let err = jobs.all_ready(&[JOB_ID_B]).expect_err("expected an error");
    assert!(
        matches!(err, AsvoError::NoAsvoJob(job_id) if job_id == JOB_ID_B),
        "got {err:?}"
    );
}

#[test]
fn all_ready_reports_a_failed_job_with_its_error() {
    let jobs = AsvoJobVec(vec![job(
        JOB_ID_A,
        AsvoJobState::Error("the conversion failed".to_string()),
    )]);
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
    let mut failed = job(
        JOB_ID_A,
        AsvoJobState::Error("the conversion failed".to_string()),
    );
    failed.error_code = Some(7);
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
        format!("MWA ASVO job ID {JOB_ID_A} (obsid: {OBS_ID}) has an error (code 7): the conversion failed")
    );

    failed.error_code = None;
    let err = AsvoJobVec(vec![failed])
        .all_ready(&[JOB_ID_A])
        .expect_err("the job has failed");
    assert_eq!(
        err.to_string(),
        format!("MWA ASVO job ID {JOB_ID_A} (obsid: {OBS_ID}) has an error: the conversion failed")
    );
}

#[test]
fn all_ready_reports_an_expired_or_cancelled_job() {
    let expired = AsvoJobVec(vec![job(JOB_ID_A, AsvoJobState::Expired)]);
    assert!(matches!(
        expired.all_ready(&[JOB_ID_A]),
        Err(AsvoError::JobExpired(JOB_ID_A))
    ));

    let cancelled = AsvoJobVec(vec![job(JOB_ID_A, AsvoJobState::Cancelled)]);
    assert!(matches!(
        cancelled.all_ready(&[JOB_ID_A]),
        Err(AsvoError::JobCancelled(JOB_ID_A))
    ));
}

#[test]
fn all_ready_reports_the_first_failure_in_the_order_asked() {
    let jobs = AsvoJobVec(vec![
        job(JOB_ID_A, AsvoJobState::Expired),
        job(JOB_ID_B, AsvoJobState::Cancelled),
    ]);
    assert!(matches!(
        jobs.all_ready(&[JOB_ID_B, JOB_ID_A]),
        Err(AsvoError::JobCancelled(JOB_ID_B))
    ));
}

/// A job with the given ID, obsid, type and state, for the filter tests.
fn job_with(job_id: AsvoJobId, obs_id: u64, job_type: AsvoJobType, state: AsvoJobState) -> AsvoJob {
    AsvoJob {
        obs_id: ObsId::validate(obs_id).expect("the test obsid should be valid"),
        job_id,
        job_type,
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
    }
}

/// Three jobs that differ in job ID, obsid, type and state.
fn mixed_jobs() -> AsvoJobVec {
    AsvoJobVec(vec![
        job_with(
            JOB_ID_A,
            OBS_ID,
            AsvoJobType::Conversion,
            AsvoJobState::Ready,
        ),
        job_with(
            JOB_ID_B,
            OTHER_OBS_ID,
            AsvoJobType::DownloadVisibilities,
            AsvoJobState::Error("failed".to_string()),
        ),
        job_with(
            JOB_ID_C,
            OBS_ID,
            AsvoJobType::DownloadMetadata,
            AsvoJobState::Queued,
        ),
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
    let jobs = mixed_jobs().filter(&[], &[], &[AsvoJobType::DownloadMetadata], &[]);
    assert_eq!(ids(&jobs), vec![JOB_ID_C]);
}

#[test]
fn filter_by_state_matches_any_error() {
    let jobs = mixed_jobs().filter(
        &[],
        &[],
        &[],
        &[AsvoJobState::Error(String::new()), AsvoJobState::Ready],
    );
    assert_eq!(ids(&jobs), vec![JOB_ID_A, JOB_ID_B]);
}

#[test]
fn filter_criteria_are_combined() {
    let obs_id = ObsId::validate(OBS_ID).expect("the test obsid should be valid");
    let jobs = mixed_jobs().filter(&[], &[obs_id], &[], &[AsvoJobState::Queued]);
    assert_eq!(ids(&jobs), vec![JOB_ID_C]);
}

/// `giant-squid list --json` and `AsvoJobVec::json` use the OpenAPI names
/// (decision 10). `--legacy-json` keeps the old keys; see `cli::legacy_json`.
#[test]
fn the_json_output_keys_are_the_openapi_names() {
    let mut ready = job(JOB_ID_A, AsvoJobState::Ready);
    ready.product = Some(AsvoJobProduct {
        files: vec![AsvoFilesArray {
            r#type: Delivery::Acacia,
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
            "completed",
            "created",
            "error_code",
            "error_text",
            "first_name",
            "job_id",
            "job_params",
            "job_state",
            "job_type",
            "last_name",
            "modified",
            "obs_id",
            "product",
            "started",
            "user_id"
        ]
    );
    let file_keys: Vec<&str> = entry["product"]["files"][0]
        .as_object()
        .expect("each file is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(file_keys, ["format", "path", "sha1", "size", "type", "url"]);
}
