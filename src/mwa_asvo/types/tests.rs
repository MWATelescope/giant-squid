// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super`] MWA ASVO data types.

use super::*;
use crate::mwa_asvo::api::openapi::{DeliveryDestination, JobProduct};
use crate::test_config::job_type;

/// An obsid used by these tests.
const OBS_ID: u64 = 1065880128;
/// Job IDs used by these tests.
const JOB_ID_A: MwaAsvoJobId = crate::test_config::job_id(101);
const JOB_ID_B: MwaAsvoJobId = crate::test_config::job_id(102);
const JOB_ID_C: MwaAsvoJobId = crate::test_config::job_id(103);
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

/// The job type names are the values of the schema's `JobType`, in its order.
#[test]
fn the_job_type_names_are_the_schema_values() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    let values: Vec<&str> = schema["definitions"]["JobType"]["enum"]
        .as_array()
        .expect("JobType is an enum")
        .iter()
        .map(|value| value.as_str().expect("a JobType value is text"))
        .collect();
    assert_eq!(JobType::names(), values);
}

/// Each name parses to its type, in any case and spelling of the separators.
#[test]
fn a_job_type_is_parsed_from_its_name() {
    for name in JobType::names() {
        let job_type = JobType::parse_name(&name).expect("a listed name should parse");
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
            matches!(&err, MwaAsvoError::InvalidJobType { str } if str == text),
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
            matches!(&err, MwaAsvoError::InvalidJobState { str } if str == text),
            "{text}: {err:?}"
        );
    }
}

/// A job with the given ID and state.
fn job(job_id: MwaAsvoJobId, state: JobState) -> MwaAsvoJob {
    crate::test_config::mwa_asvo_job(
        ObsId::validate(OBS_ID).expect("the test obsid should be valid"),
        JobDetailResponse {
            job_id,
            obs_id: 0,
            job_type: job_type("visibility"),
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
            job_params: Default::default(),
        },
    )
}

#[test]
fn all_ready_is_true_when_every_job_is_ready() {
    let jobs = MwaAsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Completed),
    ]);
    assert!(jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_is_false_while_a_job_is_in_progress() {
    let jobs = MwaAsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Queued),
    ]);
    assert!(!jobs
        .all_ready(&[JOB_ID_A, JOB_ID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_ignores_jobs_that_were_not_asked_about() {
    let jobs = MwaAsvoJobVec(vec![
        job(JOB_ID_A, JobState::Completed),
        job(JOB_ID_B, JobState::Cancelled),
    ]);
    assert!(jobs.all_ready(&[JOB_ID_A]).expect("no job has failed"));
}

#[test]
fn all_ready_reports_an_unknown_job() {
    let jobs = MwaAsvoJobVec(vec![job(JOB_ID_A, JobState::Completed)]);
    let err = jobs.all_ready(&[JOB_ID_B]).expect_err("expected an error");
    assert!(
        matches!(err, MwaAsvoError::NoMwaAsvoJob(job_id) if job_id == JOB_ID_B),
        "got {err:?}"
    );
}

#[test]
fn all_ready_reports_a_failed_job_with_its_error() {
    let mut failed = job(JOB_ID_A, JobState::Error);
    failed.detail_mut().error_text = Some("the conversion failed".to_string());
    let jobs = MwaAsvoJobVec(vec![failed]);
    match jobs.all_ready(&[JOB_ID_A]) {
        Err(MwaAsvoError::JobFailed {
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
    let jobs = MwaAsvoJobVec(vec![failed.clone()]);

    let err = jobs.all_ready(&[JOB_ID_A]).expect_err("the job has failed");
    assert!(
        matches!(
            &err,
            MwaAsvoError::JobFailed {
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
    let err = MwaAsvoJobVec(vec![failed])
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
    let cancelled = MwaAsvoJobVec(vec![job(JOB_ID_A, JobState::Cancelled)]);
    assert!(matches!(
        cancelled.all_ready(&[JOB_ID_A]),
        Err(MwaAsvoError::JobCancelled(JOB_ID_A))
    ));
}

#[test]
fn all_ready_reports_the_first_failure_in_the_order_asked() {
    let jobs = MwaAsvoJobVec(vec![
        job(JOB_ID_A, JobState::Error),
        job(JOB_ID_B, JobState::Cancelled),
    ]);
    assert!(matches!(
        jobs.all_ready(&[JOB_ID_B, JOB_ID_A]),
        Err(MwaAsvoError::JobCancelled(JOB_ID_B))
    ));
}

/// A job with the given ID, obsid, type and state, for the filter tests.
fn job_with(job_id: MwaAsvoJobId, obs_id: u64, job_type: JobType, state: JobState) -> MwaAsvoJob {
    crate::test_config::mwa_asvo_job(
        ObsId::validate(obs_id).expect("the test obsid should be valid"),
        JobDetailResponse {
            job_id,
            obs_id: 0,
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
            job_params: Default::default(),
        },
    )
}

/// Three jobs that differ in job ID, obsid, type and state.
fn mixed_jobs() -> MwaAsvoJobVec {
    MwaAsvoJobVec(vec![
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
fn ids(jobs: &MwaAsvoJobVec) -> Vec<MwaAsvoJobId> {
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

/// `giant-squid list --json` and `MwaAsvoJobVec::json` give each job as the
/// schema's `JobDetailResponse`, plus `obs_id`. A field with no value (here
/// `started`, `completed` and the others) has no key, as in the schema.
/// `--legacy-json` keeps the old keys; see `cli::legacy_json`.
#[test]
fn the_json_output_keys_are_the_openapi_names() {
    let mut ready = job(JOB_ID_A, JobState::Completed);
    ready.detail_mut().product = Some(JobProduct {
        files: vec![JobFile {
            type_: DeliveryDestination::Acacia,
            url: Some("https://example.org/f.tar".to_string()),
            path: None,
            size: 1,
            sha1: Some("0".repeat(40)),
            format: None,
        }],
    });

    let json: serde_json::Value = serde_json::from_str(
        &MwaAsvoJobVec(vec![ready])
            .json()
            .expect("the jobs serialise"),
    )
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
            "job_id",
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

// ---------------------------------------------------------------------------
// Download links that have expired
// ---------------------------------------------------------------------------

/// A signed URL of the MWA ASVO, as the file server gives it: `Expires` is
/// when the signature stops working (seconds since the Unix epoch).
const SIGNED_URL: &str = "https://projects.pawsey.org.au/mwa-asvo/1475788520_1112133_image.tar?AWSAccessKeyId=a5e466f891734d45a67676504a309c35&Signature=vWSo92d14rykcu404LjnWNP4j8Q%3D&Expires=1792380172";
/// `Expires` of [`SIGNED_URL`].
const SIGNED_URL_EXPIRES: &str = "2026-10-19T03:22:52Z";

fn time(text: &str) -> jiff::Timestamp {
    text.parse().expect("a valid time")
}

/// A completed job with the given files: each is a delivery type and a URL.
fn completed_job_with(files: &[(DeliveryDestination, Option<&str>)]) -> MwaAsvoJob {
    let mut job = job(JOB_ID_A, JobState::Completed);
    job.detail_mut().product = Some(JobProduct {
        files: files
            .iter()
            .map(|(type_, url)| JobFile {
                type_: *type_,
                url: url.map(str::to_string),
                path: None,
                size: 1,
                sha1: Some("0".repeat(40)),
                format: None,
            })
            .collect(),
    });
    job
}

#[test]
fn the_expiry_of_a_signed_url_is_its_expires_value() {
    assert_eq!(
        signed_url_expiry(SIGNED_URL),
        Some(time(SIGNED_URL_EXPIRES))
    );
    // The other query keys, and their order, do not matter.
    assert_eq!(
        signed_url_expiry("https://example.org/f.tar?Expires=1792380172&Signature=x"),
        Some(time(SIGNED_URL_EXPIRES))
    );
}

#[test]
fn a_url_with_no_usable_expires_has_no_expiry() {
    for url in [
        "https://example.org/f.tar",
        "https://example.org/f.tar?Signature=x",
        "https://example.org/f.tar?Expires=",
        "https://example.org/f.tar?Expires=soon",
        "https://example.org/f.tar?Expires=-1x",
        "https://example.org/f.tar?expires=1792380172",
        "not a url",
    ] {
        assert_eq!(signed_url_expiry(url), None, "{url}");
    }
}

#[test]
fn the_expiry_of_a_job_is_the_earliest_of_its_acacia_urls() {
    let later = "https://example.org/b.tar?Expires=1792380172";
    let earlier = "https://example.org/a.tar?Expires=1792300000";
    let job = completed_job_with(&[
        (DeliveryDestination::Acacia, Some(later)),
        (DeliveryDestination::Acacia, Some(earlier)),
        // Only the files of an Acacia job have a signed URL.
        (
            DeliveryDestination::Scratch,
            Some("https://example.org/c.tar?Expires=1"),
        ),
    ]);

    assert_eq!(job.expires(), Some(time("2026-10-18T05:06:40Z")));
}

#[test]
fn a_job_with_no_acacia_url_has_no_expiry() {
    let no_url = completed_job_with(&[(DeliveryDestination::Acacia, None)]);
    let scratch = completed_job_with(&[(DeliveryDestination::Scratch, None)]);
    let no_product = job(JOB_ID_A, JobState::Completed);

    for job in [no_url, scratch, no_product] {
        assert_eq!(job.expires(), None);
        assert!(!job.is_expired_at(time("2100-01-01T00:00:00Z")));
        assert_eq!(
            job.display_state_at(time("2100-01-01T00:00:00Z")),
            "completed (ready)"
        );
    }
}

#[test]
fn a_job_is_expired_from_the_time_in_expires() {
    let job = completed_job_with(&[(DeliveryDestination::Acacia, Some(SIGNED_URL))]);

    assert!(!job.is_expired_at(time("2026-10-19T03:22:51Z")));
    assert!(job.is_expired_at(time(SIGNED_URL_EXPIRES)));
    assert!(job.is_expired_at(time("2026-10-20T00:00:00Z")));
    // A URL that expired a day ago is expired. It does not wait for a week
    // after `Expires`: `Expires` is the end of the week that the URL lasts.
    assert!(job.is_expired_at(time("2026-10-19T03:22:53Z")));
}

#[test]
fn only_a_completed_job_is_expired() {
    for state in [JobState::Queued, JobState::Error, JobState::Cancelled] {
        let mut job = completed_job_with(&[(DeliveryDestination::Acacia, Some(SIGNED_URL))]);
        job.detail_mut().job_state = state;

        assert!(!job.is_expired_at(time("2100-01-01T00:00:00Z")), "{state}");
        assert_eq!(
            job.display_state_at(time("2100-01-01T00:00:00Z")),
            state.to_string()
        );
    }
}

#[test]
fn the_display_state_of_a_completed_job_says_ready_or_expired() {
    let job = completed_job_with(&[(DeliveryDestination::Acacia, Some(SIGNED_URL))]);

    assert_eq!(
        job.display_state_at(time("2026-10-12T00:00:00Z")),
        "completed (ready)"
    );
    assert_eq!(
        job.display_state_at(time("2026-10-20T00:00:00Z")),
        "completed (expired)"
    );
    // The state of the job is still the API's.
    assert_eq!(job.job_state, JobState::Completed);
}

#[test]
fn all_ready_reports_a_completed_job_whose_download_links_have_expired() {
    let jobs = MwaAsvoJobVec(vec![completed_job_with(&[(
        DeliveryDestination::Acacia,
        Some(SIGNED_URL),
    )])]);

    assert!(jobs
        .all_ready_at(&[JOB_ID_A], time("2026-10-12T00:00:00Z"))
        .expect("the links still work"));
    let err = jobs
        .all_ready_at(&[JOB_ID_A], time("2026-10-20T00:00:00Z"))
        .expect_err("the links have expired");
    assert!(
        matches!(
            &err,
            MwaAsvoError::JobExpired { job_id, expired }
                if *job_id == JOB_ID_A && *expired == Some(time(SIGNED_URL_EXPIRES))
        ),
        "{err:?}"
    );
}

/// The text of a job does not have the signed URLs of its files: one works
/// as a password for 7 days.
#[test]
fn the_text_of_a_job_has_no_signed_url() {
    let signature = "Signature=aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA";
    let url = format!("https://example.org/a.tar?Expires=1792380172&{signature}");
    let job = completed_job_with(&[(DeliveryDestination::Acacia, Some(&url))]);

    let text = job.to_string();

    assert!(!text.contains(signature), "{text}");
    assert!(text.contains("files: 1"), "{text}");
}

/// The size of each file of the jobs of the `total_size` test, in bytes.
const FILE_SIZES: [i64; 2] = [1000, 2345];

/// A completed job whose files have the sizes `sizes`.
fn completed_job_with_sizes(sizes: &[i64]) -> MwaAsvoJob {
    let mut job = completed_job_with(&vec![(DeliveryDestination::Acacia, None); sizes.len()]);
    if let Some(product) = job.detail_mut().product.as_mut() {
        for (file, size) in product.files.iter_mut().zip(sizes) {
            file.size = *size;
        }
    }
    job
}

#[test]
fn the_total_size_is_the_sum_of_the_file_sizes() {
    let job = completed_job_with_sizes(&FILE_SIZES);
    assert_eq!(job.total_size(), Some(3345));
}

#[test]
fn a_job_with_no_product_has_no_total_size() {
    assert_eq!(job(JOB_ID_A, JobState::Queued).total_size(), None);
}

#[test]
fn a_product_with_no_files_has_a_total_size_of_zero() {
    assert_eq!(completed_job_with(&[]).total_size(), Some(0));
}

#[test]
fn a_negative_file_size_counts_as_zero_in_the_total_size() {
    let job = completed_job_with_sizes(&[-5, FILE_SIZES[0]]);
    assert_eq!(job.total_size(), Some(1000));
}

#[test]
fn the_download_destination_of_an_acacia_job_is_its_job_directory() {
    let job = completed_job_with(&[(DeliveryDestination::Acacia, None)]);
    let download_dir = std::path::Path::new("/data");

    assert_eq!(
        job.download_destination(download_dir, true),
        download_dir.join(JOB_ID_A.to_string())
    );
    assert_eq!(job.download_destination(download_dir, false), download_dir);
}

#[test]
fn the_download_destination_of_a_scratch_job_is_the_download_directory() {
    let job = completed_job_with(&[(DeliveryDestination::Scratch, None)]);
    let download_dir = std::path::Path::new("/data");

    assert_eq!(job.download_destination(download_dir, true), download_dir);
    assert_eq!(job.download_destination(download_dir, false), download_dir);
}

#[test]
fn the_download_destination_of_a_job_with_no_files_is_its_job_directory() {
    let job = job(JOB_ID_A, JobState::Queued);
    let download_dir = std::path::Path::new("/data");

    assert_eq!(
        job.download_destination(download_dir, true),
        download_dir.join(JOB_ID_A.to_string())
    );
}
