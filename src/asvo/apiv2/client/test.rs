// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the MWA ASVO API client, run against a local mock server.
//!
//! These cover the paths a recording cannot: authentication, token refresh,
//! rejected tokens, and server error responses. They submit nothing to a
//! real MWA ASVO and download nothing from Acacia. The playback tests at the
//! end replay a recording captured from a live server, and the one recording
//! test is `#[ignore]`d. See docs/TESTING.md.

use clap::Parser;
use httpmock::prelude::*;
use serde_json::json;

use crate::asvo::apiv2::openapi::DownloadJobParams;
use crate::asvo::{AsvoApiError, AsvoClient, AsvoJobState, AsvoJobType, Delivery};
use crate::cli::Args;
use crate::test_common::*;

/// Whether `err` is an API error carrying the given machine-readable code.
fn is_api_error(err: &AsvoApiError, code: &str) -> bool {
    match err {
        AsvoApiError::ApiError { error_code, .. } => error_code.as_str() == code,
        _ => false,
    }
}

/// Parse a CLI invocation and build the visibility download body it implies,
/// so these tests exercise the same path a user's command line takes.
fn vis_params_from_cli(args: &[&str]) -> DownloadJobParams {
    match Args::try_parse_from(args).expect("arguments should parse") {
        Args::SubmitVis { download, .. } => download
            .to_vis_params(TEST_OBSID_I64)
            .expect("params should build"),
        _ => panic!("expected submit-vis"),
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[test]
fn a_missing_api_key_is_reported_before_any_request() {
    let _env = TestEnv::without_api_key();

    let err = AsvoClient::new().expect_err("expected a missing-key failure");
    assert!(matches!(err, AsvoApiError::MissingAuthKey), "got {err:?}");
}

#[test]
fn a_valid_cached_session_is_reused_without_logging_in() {
    let env = TestEnv::with_session();
    let login = env.mock_login();
    let get_jobs = env.mock_get_jobs(vec![]);

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client.get_jobs(None).expect("get_jobs should succeed");

    assert!(jobs.0.is_empty());
    assert_eq!(login.calls(), 0, "a cached session should not log in");
    assert_eq!(get_jobs.calls(), 1);
}

#[test]
fn a_fresh_login_is_performed_and_cached_when_no_session_exists() {
    let env = TestEnv::without_session();
    let login = env.mock_login();

    AsvoClient::new().expect("client should be created");

    assert_eq!(login.calls(), 1);
    let cached = env.cached_session().expect("the session should be cached");
    assert_eq!(cached["user_login"], TEST_USER_LOGIN);
    assert_eq!(cached["user_id"], TEST_USER_ID);
}

#[test]
fn a_rejected_login_is_reported_as_an_authentication_failure() {
    let env = TestEnv::without_session();
    let login = env.mock_login_failure(401, "invalid api key");

    let err = AsvoClient::new().expect_err("expected the login to fail");

    assert_eq!(login.calls(), 1);
    match err {
        AsvoApiError::AuthenticationFailed { message } => {
            assert!(message.contains("invalid api key"), "got {message}");
        }
        other => panic!("expected AuthenticationFailed, got {other:?}"),
    }
    assert!(
        env.cached_session().is_none(),
        "a failed login must not be cached"
    );
}

#[test]
fn an_expired_access_token_is_refreshed_rather_than_re_logged_in() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-60), jwt_expiring_in(86400));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/refresh");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(token_response());
    });

    AsvoClient::new().expect("client should be created");

    assert_eq!(refresh.calls(), 1);
    assert_eq!(login.calls(), 0, "a valid refresh token should be used");
}

#[test]
fn a_failed_refresh_falls_back_to_a_fresh_login() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-60), jwt_expiring_in(86400));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/refresh");
        then.status(401).body("refresh token already rotated");
    });

    AsvoClient::new().expect("client should still be created");

    assert_eq!(refresh.calls(), 1);
    assert_eq!(login.calls(), 1);
}

#[test]
fn an_expired_refresh_token_goes_straight_to_a_fresh_login() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-120), jwt_expiring_in(-60));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/refresh");
        then.status(200).json_body(token_response());
    });

    AsvoClient::new().expect("client should be created");

    assert_eq!(refresh.calls(), 0);
    assert_eq!(login.calls(), 1);
}

#[test]
fn a_token_the_server_rejects_triggers_one_relogin_and_one_retry() {
    let env = TestEnv::with_session();
    let login = env.mock_login();
    let get_jobs = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/get_jobs");
        then.status(401)
            .header("content-type", "application/json")
            .json_body(error_response(
                "AUTH_INVALID_TOKEN",
                "Access token is invalid",
            ));
    });

    let client = AsvoClient::new().expect("client should be created");
    let Err(err) = client.get_jobs(None) else {
        panic!("expected the call to fail");
    };

    assert!(is_api_error(&err, "AUTH_INVALID_TOKEN"), "got {err:?}");
    assert_eq!(
        login.calls(),
        1,
        "the rejected token should force a re-login"
    );
    assert_eq!(get_jobs.calls(), 2, "the request should be retried once");
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

#[test]
fn a_structured_error_body_becomes_an_api_error() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/get_jobs");
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("JOB_INVALID_STATE", "Job is not ready"));
    });

    let client = AsvoClient::new().expect("client should be created");
    let Err(err) = client.get_jobs(None) else {
        panic!("expected the call to fail");
    };

    match err {
        AsvoApiError::ApiError {
            error_code,
            message,
            suggestion,
            ..
        } => {
            assert_eq!(error_code, "JOB_INVALID_STATE");
            assert_eq!(message, "Job is not ready");
            assert_eq!(suggestion.as_deref(), Some("try something else"));
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

#[test]
fn a_non_json_error_body_becomes_a_bad_status() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/get_jobs");
        then.status(502).body("<html>bad gateway</html>");
    });

    let client = AsvoClient::new().expect("client should be created");
    let Err(err) = client.get_jobs(None) else {
        panic!("expected the call to fail");
    };

    match err {
        AsvoApiError::BadStatus { code, message } => {
            assert_eq!(code.as_u16(), 502);
            assert!(message.contains("bad gateway"), "got {message}");
        }
        other => panic!("expected BadStatus, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Job listing
// ---------------------------------------------------------------------------

#[test]
fn a_job_listing_is_mapped_from_the_api_response() {
    let env = TestEnv::with_session();
    let get_jobs = env.mock_get_jobs(vec![job_detail(
        TEST_JOBID as i64,
        TEST_OBSID,
        "completed",
        1,
    )]);

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client.get_jobs(None).expect("get_jobs should succeed");

    assert_eq!(get_jobs.calls(), 1);
    assert_eq!(jobs.0.len(), 1);
    let job = &jobs.0[0];
    assert_eq!(job.jobid, TEST_JOBID);
    assert_eq!(job.obsid.get(), TEST_OBSID_I64 as u64);
    assert_eq!(job.jtype, AsvoJobType::DownloadVisibilities);
    // The API says "completed" where the rest of giant-squid says "ready".
    assert_eq!(job.state, AsvoJobState::Ready);
}

/// The listing endpoint is paged 100 at a time, so a larger history has to
/// be walked. Each page is matched on the `offset` the client sends.
#[test]
fn a_long_job_listing_is_fetched_page_by_page() {
    let env = TestEnv::with_session();
    let total = 150;
    let page = |from: i64, count: i64| -> Vec<serde_json::Value> {
        (from..from + count)
            .map(|id| job_detail(id, TEST_OBSID, "completed", 1))
            .collect()
    };

    let first = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/get_jobs")
            .json_body_includes(r#"{ "offset": 0 }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "jobs": page(1, 100), "total_count": total }));
    });
    let second = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/get_jobs")
            .json_body_includes(r#"{ "offset": 100 }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "jobs": page(101, 50), "total_count": total }));
    });

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client.get_jobs(None).expect("get_jobs should succeed");

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1, "the second page should be requested");
    assert_eq!(jobs.0.len(), total as usize);
    assert_eq!(jobs.0.last().expect("there should be jobs").jobid, 150);
}

#[test]
fn an_errored_job_carries_the_servers_error_text() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOBID as i64, TEST_OBSID, "error", 0);
    detail["error_text"] = json!("Observation has no data files");
    env.mock_get_jobs(vec![detail]);

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client.get_jobs(None).expect("get_jobs should succeed");

    assert_eq!(
        jobs.0[0].state,
        AsvoJobState::Error("Observation has no data files".to_string())
    );
    assert_eq!(jobs.0[0].jtype, AsvoJobType::Conversion);
}

#[test]
fn unusable_jobs_are_skipped_rather_than_failing_the_listing() {
    let env = TestEnv::with_session();

    let mut no_obsid = job_detail(1, TEST_OBSID, "completed", 1);
    no_obsid["job_params"] = json!({ "delivery": "acacia" });

    let unknown_state = job_detail(2, TEST_OBSID, "wibble", 1);

    let mut bad_obsid = job_detail(3, TEST_OBSID, "completed", 1);
    bad_obsid["job_params"] = json!({ "obs_id": "42" });

    let good = job_detail(TEST_JOBID as i64, TEST_OBSID, "queued", 1);

    env.mock_get_jobs(vec![no_obsid, unknown_state, bad_obsid, good]);

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client.get_jobs(None).expect("get_jobs should succeed");

    assert_eq!(jobs.0.len(), 1, "only the usable job should be returned");
    assert_eq!(jobs.0[0].jobid, TEST_JOBID);
    assert_eq!(jobs.0[0].state, AsvoJobState::Queued);
}

// ---------------------------------------------------------------------------
// Submission and cancellation
// ---------------------------------------------------------------------------

#[test]
fn a_visibility_job_posts_the_body_the_cli_built() {
    let env = TestEnv::with_session();
    let params = vis_params_from_cli(&["giant-squid", "submit-vis", TEST_OBSID]);
    let expected = serde_json::to_value(&params).expect("params should serialise");

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body(expected);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(777));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .submit_download_vis_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 777);
}

#[test]
fn a_metadata_job_posts_to_the_same_endpoint_with_a_meta_download_type() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-meta", TEST_OBSID])
        .expect("arguments should parse")
    {
        Args::SubmitMeta { download, .. } => download
            .to_meta_params(TEST_OBSID_I64)
            .expect("params should build"),
        _ => panic!("expected submit-meta"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(r#"{ "download_type": "meta" }"#);
        then.status(200).json_body(job_submitted_response(778));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .submit_download_vis_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 778);
}

#[test]
fn a_conversion_job_posts_to_the_conversion_endpoint() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-conv", TEST_OBSID])
        .expect("arguments should parse")
    {
        Args::SubmitConv { conv, .. } => {
            conv.to_params(TEST_OBSID_I64).expect("params should build")
        }
        _ => panic!("expected submit-conv"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/conversion_job")
            .json_body_includes(format!(r#"{{ "obs_id": {TEST_OBSID_I64} }}"#));
        then.status(200).json_body(job_submitted_response(779));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .submit_conversion_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 779);
}

#[test]
fn a_cancellation_deletes_the_job_resource() {
    let env = TestEnv::with_session();
    let cancel = env.server.mock(|when, then| {
        when.method(DELETE)
            .path(format!("/api/v2/jobs/{TEST_JOBID}"));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(TEST_JOBID as u64));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .cancel_job(TEST_JOBID)
        .expect("cancellation should succeed");

    assert_eq!(cancel.calls(), 1);
    assert_eq!(resp.job_id.get(), TEST_JOBID as u64);
    assert_eq!(resp.message, "Job submitted");
}

#[test]
fn a_cancellation_of_an_unknown_job_is_reported() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/999999");
        then.status(404)
            .header("content-type", "application/json")
            .json_body(error_response("JOB_NOT_FOUND", "No such job"));
    });

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .cancel_job(999999)
        .expect_err("expected the cancellation to fail");

    assert!(is_api_error(&err, "JOB_NOT_FOUND"), "got {err:?}");
}

/// Imaging submissions return a `JobSubmittedResponse`, like every other
/// v2 submit endpoint, not a bare job ID.
#[test]
fn an_imaging_job_returns_a_job_submitted_response() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-image", TEST_OBSID])
        .expect("arguments should parse")
    {
        Args::SubmitImage { image, .. } => image
            .to_params(TEST_OBSID_I64)
            .expect("params should build"),
        _ => panic!("expected submit-image"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/imaging_job");
        then.status(200).json_body(job_submitted_response(779));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .submit_imaging_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 779);
}

#[test]
fn an_image_from_job_submission_returns_a_job_submitted_response() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from([
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "12345",
        TEST_OBSID,
    ])
    .expect("arguments should parse")
    {
        Args::SubmitImageFromJob { image, .. } => image
            .to_params(TEST_OBSID_I64)
            .expect("params should build"),
        _ => panic!("expected submit-image-from-job"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/image_from_job");
        then.status(200).json_body(job_submitted_response(780));
    });

    let client = AsvoClient::new().expect("client should be created");
    let resp = client
        .submit_image_from_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 780);
}

// ---------------------------------------------------------------------------
// Playback of a recording captured from a live MWA ASVO
// ---------------------------------------------------------------------------
//
// `tests/fixtures/login_and_get_jobs.yaml` was recorded against test-asvo by
// `record_login_and_get_jobs` below and scrubbed by
// `tools/scrub_recording.py`. Loading it into a mock server turns the
// recorded requests into matching criteria, so these tests assert the
// client's behaviour against a real server response rather than one this
// repo invented.
//
// The fixture also holds the login exchange, but these tests write a cached
// session instead of replaying it. The recorded login request body carries
// the client version (`giant-squidv3.0.0`), so a version bump would stop it
// matching and break the tests for an unrelated reason. It is kept in the
// fixture for reference and for manual use.

/// Values from the recorded response, so a re-record that changes them
/// fails loudly here rather than silently weakening the test.
const RECORDED_JOB_ID: u32 = 30000517;
const RECORDED_OBSID: u64 = 1115977528;
const RECORDED_SIZE: u64 = 117016360960;
const RECORDED_SHA1: &str = "ce32e0aeec0b7c64dec4deeb89881ba4452a6330";

#[test]
fn a_recorded_job_listing_is_mapped_as_expected() {
    let env = TestEnv::with_session();
    env.server.playback(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("login_and_get_jobs.yaml"),
    );

    let client = AsvoClient::new().expect("client should be created");
    // The recording was made with --days 30, and the recorded request is
    // the matching criteria, so the same argument is required here.
    let jobs = client
        .get_jobs(Some(30))
        .expect("the recorded listing should be served");

    assert_eq!(jobs.0.len(), 1);
    let job = &jobs.0[0];
    assert_eq!(job.jobid, RECORDED_JOB_ID);
    assert_eq!(job.obsid.get(), RECORDED_OBSID);
    assert_eq!(job.jtype, AsvoJobType::DownloadMetadata);
    assert_eq!(job.state, AsvoJobState::Ready);
    assert!(job.completed.is_some(), "completed should be parsed");
}

/// The point of the recording: `product` is a free-form object in the
/// schema, so this pins the mapping against a real payload.
#[test]
fn a_recorded_jobs_product_becomes_a_file_list() {
    let env = TestEnv::with_session();
    env.server.playback(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("login_and_get_jobs.yaml"),
    );

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client
        .get_jobs(Some(30))
        .expect("the recorded listing should be served");

    let files = jobs.0[0]
        .files
        .as_ref()
        .expect("a completed job should carry its files");
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.r#type, Delivery::Acacia);
    assert_eq!(file.size, RECORDED_SIZE);
    assert_eq!(file.sha1.as_deref(), Some(RECORDED_SHA1));
    assert!(
        file.url
            .as_deref()
            .unwrap_or_default()
            .contains("1115977528_30000517_meta.tar"),
        "the signed download URL should be carried through: {:?}",
        file.url
    );
    assert!(file.path.is_none(), "an Acacia file has no filesystem path");
}

// ---------------------------------------------------------------------------
// Recording fixtures from a live MWA ASVO
// ---------------------------------------------------------------------------
//
// This is `#[ignore]`d: CI never runs it, and it is the only test in the
// suite that talks to a real server. Run it by hand when the API changes:
//
// ```text
// HOME=$(mktemp -d) \
// MWA_ASVO_API_KEY=<your key> \
// MWA_ASVO_RECORD_TARGET=https://test-asvo.mwatelescope.org \
//   cargo test --lib record_login_and_get_jobs -- --ignored --nocapture
// ```
//
// A throwaway `HOME` is deliberate: it forces a fresh login, so the login
// exchange is captured too, and it leaves the real token cache (shared with
// mwa-cli) untouched.
//
// The recording it writes contains real JWTs, your user ID, login name and
// email. Scrub it before committing:
//
// ```text
// python3 tools/scrub_recording.py <recorded file> tests/fixtures/<name>.yaml
// ```
//
// Only read-only endpoints are recorded. Recording a submission would
// create a real job on the target server, so that is deliberately not
// automated here.

const TARGET_ENV: &str = "MWA_ASVO_RECORD_TARGET";

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn record_login_and_get_jobs() {
    let target = std::env::var(TARGET_ENV)
        .unwrap_or_else(|_| panic!("set {TARGET_ENV} to the server to record from"));

    let server = MockServer::start();
    server.forward_to(&target, |rule| {
        rule.filter(|when| {
            when.any_request();
        });
    });
    let recording = server.record(|rule| {
        rule.record_request_headers(vec!["Accept", "Content-Type"])
            .filter(|when| {
                when.any_request();
            });
    });

    // Send giant-squid's own client through the recording server. Every
    // request it makes derives its base URL from this variable.
    std::env::set_var("MWA_ASVO_HOST", server.base_url());

    let client = AsvoClient::new().expect("could not authenticate with the target server");
    let jobs = client
        .get_jobs(Some(30))
        .expect("could not list jobs on the target server");
    println!("recorded a login and a listing of {} jobs", jobs.0.len());

    let path = recording.save("login_and_get_jobs").expect("save failed");
    println!("raw recording: {}", path.display());
    println!("scrub it before committing - see the section notes in src/asvo/apiv2/client/test.rs");
}
