// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration tests for the MWA ASVO API client, run against a local
//! mock server.
//!
//! These cover the paths a recording cannot: authentication, token refresh,
//! rejected tokens, and server error responses. They submit nothing to a
//! real MWA ASVO and download nothing from Acacia. See docs/TESTING.md.

mod common;

use clap::Parser;
use common::*;
use httpmock::prelude::*;
use serde_json::json;

use mwa_giant_squid::asvo::{AsvoApiError, AsvoClient, AsvoJobState, AsvoJobType};
use mwa_giant_squid::cli::Args;

/// Whether `err` is an API error carrying the given machine-readable code.
fn is_api_error(err: &AsvoApiError, code: &str) -> bool {
    match err {
        AsvoApiError::ApiError { error_code, .. } => error_code.as_str() == code,
        _ => false,
    }
}

/// Parse a CLI invocation and build the visibility download body it implies,
/// so these tests exercise the same path a user's command line takes.
fn vis_params_from_cli(args: &[&str]) -> mwa_giant_squid::asvo::apiv2::openapi::DownloadJobParams {
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
    assert_eq!(login.calls(), 1, "the rejected token should force a re-login");
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
        Args::SubmitConv { conv, .. } => conv
            .to_params(TEST_OBSID_I64)
            .expect("params should build"),
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
        when.method(DELETE).path(format!("/api/v2/jobs/{TEST_JOBID}"));
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
