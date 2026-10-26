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

// The tests that build a request body from a command line (and the
// recording test, which reads its config as the CLI does) need the CLI, so
// they compile only with the "bin" feature. The other tests in this file
// also run without it.
#[cfg(feature = "bin")]
use clap::Parser;
use httpmock::prelude::*;
use serde_json::json;

#[cfg(feature = "bin")]
use crate::cli::Args;
use crate::mwa_asvo::api::openapi::ErrorResponse;
use crate::mwa_asvo::api::openapi::Type as FileType;
#[cfg(feature = "bin")]
use crate::mwa_asvo::api::openapi::{DownloadJobParams, JobsByUserRequest};
use crate::mwa_asvo::client_config_from_env;
use crate::mwa_asvo::{AsvoApiError, AsvoClient, AsvoError, AsvoJobId, JobState, JobsFilter};
use crate::test_common::*;
use crate::test_config::{client_config, job_type};

/// Whether `err` is an API error carrying the given machine-readable code.
fn is_api_error(err: &AsvoApiError, code: &str) -> bool {
    match err {
        AsvoApiError::ApiError { response, .. } => response.error_code == code,
        _ => false,
    }
}

/// Parse a CLI invocation and build the visibility download body it implies,
/// so these tests exercise the same path a user's command line takes.
#[cfg(feature = "bin")]
fn vis_params_from_cli(args: &[&str]) -> DownloadJobParams {
    match Args::try_parse_from(args).expect("arguments should parse") {
        Args::SubmitVis { download, .. } => {
            download.to_vis_params(crate::test_config::test_obs_id())
        }
        _ => panic!("expected submit-vis"),
    }
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

/// Compiles only if `T` is `Send` and `Sync`.
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn the_client_can_be_shared_between_threads() {
    // PyO3 requires a `#[pyclass]` to be `Sync`, and the Python bindings
    // release the GIL during network calls.
    assert_send_sync::<AsvoClient>();
}

#[test]
fn one_client_can_be_used_from_several_threads_at_once() {
    let env = TestEnv::with_session();
    let get_jobs = env.mock_get_jobs(vec![]);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                client
                    .get_jobs(&JobsFilter::default())
                    .expect("get_jobs should succeed")
            });
        }
    });

    assert_eq!(get_jobs.calls(), 4);
}

/// A timeout of 0 is refused before any request: the test server has no
/// login, so a login request would give another error.
#[test]
fn a_timeout_of_zero_is_refused_before_any_request() {
    let env = TestEnv::without_session();
    let mut config = client_config(&env);
    config.api_timeout = std::time::Duration::ZERO;

    let err = AsvoClient::new(config).expect_err("a timeout of 0 seconds");

    assert_eq!(
        err.to_string(),
        "Invalid api_timeout: must be more than 0 seconds"
    );
    assert!(matches!(err, AsvoError::InvalidOption { .. }), "{err:?}");
}

#[test]
fn a_missing_api_key_is_reported_before_any_request() {
    let env = TestEnv::without_api_key();

    let err = AsvoClient::new(client_config(&env)).expect_err("expected a missing-key failure");
    assert!(
        matches!(
            err,
            AsvoError::AsvoApi(AsvoApiError::MissingAuthKey { variable: None })
        ),
        "got {err:?}"
    );
}

#[test]
fn a_valid_cached_session_is_reused_without_logging_in() {
    let env = TestEnv::with_session();
    let login = env.mock_login();
    let get_jobs = env.mock_get_jobs(vec![]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    assert!(jobs.0.is_empty());
    assert_eq!(login.calls(), 0, "a cached session should not log in");
    assert_eq!(get_jobs.calls(), 1);
}

#[test]
fn a_fresh_login_is_performed_and_cached_when_no_session_exists() {
    let env = TestEnv::without_session();
    let login = env.mock_login();

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(login.calls(), 1);
    let cached = env.cached_session().expect("the session should be cached");
    assert_eq!(cached["user_login"], TEST_USER_LOGIN);
    assert_eq!(cached["user_id"], TEST_USER_ID);
}

/// A rejected login whose body is an MWA ASVO `ErrorResponse` keeps the
/// kind `AuthenticationFailed`, with the server's fields, so that its
/// detail and suggestion are shown.
#[test]
fn a_rejected_login_with_an_error_response_has_its_fields() {
    let env = TestEnv::without_session();
    let login = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/api_login");
        then.status(401)
            .header("content-type", "application/json")
            .json_body(json!({
                "error_code": "AUTH_INVALID_KEY",
                "message": "Bad API key",
                "detail": "The key was revoked",
                "suggestion": "Make a new key in your MWA ASVO profile",
                "request_id": "req-1"
            }));
    });

    let err = AsvoClient::new(client_config(&env)).expect_err("expected the login to fail");

    assert_eq!(login.calls(), 1);
    let text = err.to_string();
    match err {
        AsvoError::AsvoApi(AsvoApiError::AuthenticationFailed {
            message,
            response: Some(response),
        }) => {
            assert_eq!(message, "Bad API key");
            assert_eq!(response.error_code, "AUTH_INVALID_KEY");
            assert_eq!(response.detail.as_deref(), Some("The key was revoked"));
            assert_eq!(
                response.suggestion.as_deref(),
                Some("Make a new key in your MWA ASVO profile")
            );
            assert_eq!(response.field_errors, None);
            assert_eq!(response.request_id.as_deref(), Some("req-1"));
        }
        other => panic!("expected AuthenticationFailed, got {other:?}"),
    }
    assert!(
        text.starts_with("Authentication with MWA ASVO failed (AUTH_INVALID_KEY): Bad API key"),
        "{text}"
    );
    assert!(text.contains("Detail: The key was revoked"), "{text}");
    assert!(
        text.contains("Suggestion: Make a new key in your MWA ASVO profile"),
        "{text}"
    );
}

#[test]
fn a_rejected_login_is_reported_as_an_authentication_failure() {
    let env = TestEnv::without_session();
    let login = env.mock_login_failure(401, "invalid api key");

    let err = AsvoClient::new(client_config(&env)).expect_err("expected the login to fail");

    assert_eq!(login.calls(), 1);
    match err {
        AsvoError::AsvoApi(AsvoApiError::AuthenticationFailed { message, response }) => {
            assert!(message.contains("invalid api key"), "got {message}");
            assert_eq!(response, None);
        }
        other => panic!("expected AuthenticationFailed, got {other:?}"),
    }
    assert!(
        env.cached_session().is_none(),
        "a failed login must not be cached"
    );
}

// ---------------------------------------------------------------------------
// A cached session belongs to one host and one API key
// ---------------------------------------------------------------------------

/// The `host` and `api_key_sha256` of the cached session on disk.
fn cached_owner(env: &TestEnv) -> (String, String) {
    let session = env.cached_session().expect("a cached session");
    let text = |key: &str| {
        session[key]
            .as_str()
            .unwrap_or_else(|| panic!("the cached session has no {key}: {session}"))
            .to_string()
    };
    (text("host"), text("api_key_sha256"))
}

#[test]
fn a_fresh_login_is_cached_with_its_host_and_the_hash_of_its_key() {
    let env = TestEnv::without_session();
    env.mock_login();

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(
        cached_owner(&env),
        (env.base_url(), sha256_hex(TEST_API_KEY))
    );
    let file = std::fs::read_to_string(env.token_cache_path()).expect("the cache file");
    assert!(!file.contains(TEST_API_KEY), "the key must not be stored");
}

#[test]
fn a_cached_session_for_another_api_key_is_not_used() {
    let env = TestEnv::with_session();
    let login = env.mock_login();
    let mut config = client_config(&env);
    config.api_key = "another-api-key".to_string();

    AsvoClient::new(config).expect("client should be created");

    assert_eq!(
        login.calls(),
        1,
        "the session of another key must not be used"
    );
    assert_eq!(
        cached_owner(&env),
        (env.base_url(), sha256_hex("another-api-key")),
        "the new session replaces the old one"
    );
}

#[test]
fn a_cached_session_for_another_host_is_not_used() {
    let env = TestEnv::without_session();
    env.write_session_for(
        "https://another-host.example.org",
        TEST_API_KEY,
        jwt_expiring_in(3600),
        jwt_expiring_in(86400),
    );
    let login = env.mock_login();

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(
        login.calls(),
        1,
        "the session of another host must not be used"
    );
    assert_eq!(
        cached_owner(&env),
        (env.base_url(), sha256_hex(TEST_API_KEY))
    );
}

/// A file from mwa-cli or an older giant-squid has no host and no key hash.
#[test]
fn a_cached_session_in_the_old_format_is_replaced_by_a_fresh_login() {
    let env = TestEnv::with_session();
    let mut old = env.cached_session().expect("a cached session");
    let fields = old.as_object_mut().expect("the session is an object");
    fields.remove("host");
    fields.remove("api_key_sha256");
    std::fs::write(env.token_cache_path(), old.to_string()).expect("write the old file");
    let login = env.mock_login();

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(login.calls(), 1);
    assert_eq!(
        cached_owner(&env),
        (env.base_url(), sha256_hex(TEST_API_KEY))
    );
}

#[test]
fn a_refreshed_session_keeps_its_host_and_key_hash() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-60), jwt_expiring_in(86400));
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/refresh");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(token_response());
    });

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(
        cached_owner(&env),
        (env.base_url(), sha256_hex(TEST_API_KEY))
    );
}

#[test]
fn an_expired_access_token_is_refreshed_rather_than_re_logged_in() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-60), jwt_expiring_in(86400));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/refresh");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(token_response());
    });

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(refresh.calls(), 1);
    assert_eq!(login.calls(), 0, "a valid refresh token should be used");
}

#[test]
fn a_failed_refresh_falls_back_to_a_fresh_login() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-60), jwt_expiring_in(86400));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/refresh");
        then.status(401).body("refresh token already rotated");
    });

    AsvoClient::new(client_config(&env)).expect("client should still be created");

    assert_eq!(refresh.calls(), 1);
    assert_eq!(login.calls(), 1);
}

#[test]
fn an_expired_refresh_token_goes_straight_to_a_fresh_login() {
    let env = TestEnv::with_session();
    env.write_session(jwt_expiring_in(-120), jwt_expiring_in(-60));
    let login = env.mock_login();
    let refresh = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/refresh");
        then.status(200).json_body(token_response());
    });

    AsvoClient::new(client_config(&env)).expect("client should be created");

    assert_eq!(refresh.calls(), 0);
    assert_eq!(login.calls(), 1);
}

#[test]
fn a_token_the_server_rejects_triggers_one_relogin_and_one_retry() {
    let env = TestEnv::with_session();
    let login = env.mock_login();
    let get_jobs = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(401)
            .header("content-type", "application/json")
            .json_body(error_response(
                "AUTH_INVALID_TOKEN",
                "Access token is invalid",
            ));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let Err(err) = client.get_jobs(&JobsFilter::default()) else {
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

/// How many threads share one client in the concurrent re-login test.
const CONCURRENT_THREADS: usize = 8;

#[test]
fn threads_that_share_a_rejected_token_log_in_again_only_once() {
    let env = TestEnv::with_session();
    // Different lifetimes, so the two tokens are different strings.
    let stale = jwt_expiring_in(3600);
    let fresh = jwt_expiring_in(7200);
    env.write_session(stale.clone(), jwt_expiring_in(86400));

    let mut login_body = login_response();
    login_body["access_token"] = json!(fresh);
    let login = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/auth/api_login");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(login_body);
    });
    let rejected = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .cookie("mwa_access_token", &stale);
        then.status(401)
            .header("content-type", "application/json")
            .json_body(error_response(
                "AUTH_INVALID_TOKEN",
                "Access token is invalid",
            ));
    });
    let accepted = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .cookie("mwa_access_token", &fresh);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "jobs": [], "total_count": 0 }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let start = std::sync::Barrier::new(CONCURRENT_THREADS);
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENT_THREADS {
            scope.spawn(|| {
                start.wait();
                client
                    .get_jobs(&JobsFilter::default())
                    .expect("get_jobs should succeed");
            });
        }
    });

    assert_eq!(
        login.calls(),
        1,
        "only the first thread should log in again"
    );
    assert!(rejected.calls() >= 1, "the stale token should be rejected");
    assert_eq!(
        accepted.calls(),
        CONCURRENT_THREADS,
        "every thread should retry with the new token"
    );
}

// ---------------------------------------------------------------------------
// Job list requests that are tried again
// ---------------------------------------------------------------------------

/// A retry duration long enough that a test never reaches it.
const LONG_RETRY: std::time::Duration = std::time::Duration::from_secs(600);

/// Serve `POST /api/v2/jobs/list` with a failure of HTTP status `status`.
fn mock_get_jobs_failure(env: &TestEnv, status: u16) -> httpmock::Mock<'_> {
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(status).body("try later");
    })
}

#[test]
fn a_job_list_that_fails_with_a_503_is_tried_again_until_it_works() {
    let env = TestEnv::with_session();
    let mut failing = mock_get_jobs_failure(&env, 503);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let jobs = std::thread::scope(|scope| {
        let poll =
            scope.spawn(|| client.get_jobs_retrying(&JobsFilter::default(), LONG_RETRY, None));
        // The server recovers after the first failure.
        while failing.calls() == 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        failing.delete();
        env.mock_get_jobs(vec![]);
        poll.join().expect("the poll should not panic")
    })
    .expect("the job list should arrive after the server recovers");

    assert!(jobs.0.is_empty());
}

#[test]
fn a_job_list_error_that_is_not_transient_is_not_tried_again() {
    let env = TestEnv::with_session();
    let refused = mock_get_jobs_failure(&env, 403);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .get_jobs_retrying(&JobsFilter::default(), LONG_RETRY, None)
        .expect_err("a refusal is final");

    assert_eq!(refused.calls(), 1);
    assert!(
        matches!(
            err,
            AsvoError::AsvoApi(AsvoApiError::BadStatus { status: 403, .. })
        ),
        "{err:?}"
    );
}

#[test]
fn a_job_list_that_keeps_failing_gives_up_after_the_retry_duration() {
    let env = TestEnv::with_session();
    let failing = mock_get_jobs_failure(&env, 503);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .get_jobs_retrying(
            &JobsFilter::default(),
            std::time::Duration::from_secs(1),
            None,
        )
        .expect_err("the server never recovers");

    assert!(failing.calls() >= 2, "it should have tried again");
    assert!(
        matches!(
            err,
            AsvoError::AsvoApi(AsvoApiError::BadStatus { status: 503, .. })
        ),
        "{err:?}"
    );
}

#[test]
fn a_zero_retry_duration_gives_the_first_failure() {
    let env = TestEnv::with_session();
    let failing = mock_get_jobs_failure(&env, 503);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .get_jobs_retrying(&JobsFilter::default(), std::time::Duration::ZERO, None)
        .expect_err("no retries");

    assert_eq!(failing.calls(), 1);
    assert!(matches!(err, AsvoError::AsvoApi(_)), "{err:?}");
}

#[test]
fn a_stop_request_ends_the_retries() {
    let env = TestEnv::with_session();
    let failing = mock_get_jobs_failure(&env, 503);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .get_jobs_retrying(&JobsFilter::default(), LONG_RETRY, Some(&|| true))
        .expect_err("the caller asked to stop");

    assert_eq!(failing.calls(), 1);
    assert!(matches!(err, AsvoError::Interrupted), "{err:?}");
}

#[test]
fn only_a_failure_that_may_pass_is_transient() {
    let api_error = |status| {
        AsvoApiError::api_error(
            status,
            ErrorResponse {
                detail: None,
                error_code: "CODE".to_string(),
                field_errors: None,
                message: "message".to_string(),
                request_id: None,
                suggestion: None,
            },
        )
    };
    let bad_status = |status| AsvoApiError::BadStatus {
        status,
        message: String::new(),
    };

    for status in [429, 500, 502, 503, 504] {
        assert!(api_error(status).is_transient(), "ApiError {status}");
        assert!(bad_status(status).is_transient(), "BadStatus {status}");
    }
    for status in [400, 401, 403, 404, 409, 422] {
        assert!(!api_error(status).is_transient(), "ApiError {status}");
        assert!(!bad_status(status).is_transient(), "BadStatus {status}");
    }
    assert!(!AsvoApiError::MissingAuthKey { variable: None }.is_transient());
    assert!(!AsvoApiError::authentication_failed("no").is_transient());
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

#[test]
fn an_api_error_has_the_http_status_of_the_reply() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(409)
            .header("content-type", "application/json")
            .json_body(error_response("JOB_INVALID_STATE", "Job is not ready"));
    });
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("the call should fail");

    assert!(
        matches!(err, AsvoApiError::ApiError { status: 409, .. }),
        "{err:?}"
    );
}

#[test]
fn a_structured_error_body_becomes_an_api_error() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("JOB_INVALID_STATE", "Job is not ready"));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let Err(err) = client.get_jobs(&JobsFilter::default()) else {
        panic!("expected the call to fail");
    };

    match err {
        AsvoApiError::ApiError { response, .. } => {
            assert_eq!(response.error_code, "JOB_INVALID_STATE");
            assert_eq!(response.message, "Job is not ready");
            assert_eq!(response.suggestion.as_deref(), Some("try something else"));
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
}

#[test]
fn a_non_json_error_body_becomes_a_bad_status() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(502).body("<html>bad gateway</html>");
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let Err(err) = client.get_jobs(&JobsFilter::default()) else {
        panic!("expected the call to fail");
    };

    match err {
        AsvoApiError::BadStatus { status, message } => {
            assert_eq!(status, 502);
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
        TEST_JOB_ID as i64,
        TEST_OBS_ID,
        "completed",
        1,
    )]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    assert_eq!(get_jobs.calls(), 1);
    assert_eq!(jobs.0.len(), 1);
    let job = &jobs.0[0];
    assert_eq!(job.job_id(), crate::test_config::TEST_ASVO_JOB_ID);
    assert_eq!(job.obs_id().get(), TEST_OBS_ID_I64 as u64);
    assert_eq!(job.job_type, Some(job_type("visibility")));
    // The API says "completed" where the rest of giant-squid says "ready".
    assert_eq!(job.job_state, JobState::Completed);
}

/// The listing endpoint is paged 100 at a time, so a larger history has to
/// be walked. Each page is matched on the `offset` the client sends.
#[test]
fn a_long_job_listing_is_fetched_page_by_page() {
    let env = TestEnv::with_session();
    let total = 150;
    let page = |from: i64, count: i64| -> Vec<serde_json::Value> {
        (from..from + count)
            .map(|id| job_detail(id, TEST_OBS_ID, "completed", 1))
            .collect()
    };

    let first = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(r#"{ "offset": 0 }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "jobs": page(1, 100), "total_count": total }));
    });
    let second = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(r#"{ "offset": 100 }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "jobs": page(101, 50), "total_count": total }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1, "the second page should be requested");
    assert_eq!(jobs.0.len(), total as usize);
    assert_eq!(
        jobs.0.last().expect("there should be jobs").job_id(),
        crate::test_config::job_id(150)
    );
}

#[test]
fn an_errored_job_carries_the_servers_error_text() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "error", 0);
    detail["error_text"] = json!("Observation has no data files");
    env.mock_get_jobs(vec![detail]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    assert_eq!(jobs.0[0].job_state, JobState::Error);
    assert_eq!(
        jobs.0[0].error_text.as_deref(),
        Some("Observation has no data files")
    );
    assert_eq!(jobs.0[0].job_type, Some(job_type("conversion")));
    // The server sent no `error_code` key, which is the same as null.
    assert_eq!(jobs.0[0].error_code, None);
}

#[test]
fn unusable_jobs_are_skipped_rather_than_failing_the_listing() {
    let env = TestEnv::with_session();

    // An `obs_id` that is not an Obs ID makes the job unusable: it is
    // skipped, and the others are returned.
    let mut bad_obs_id = job_detail(3, TEST_OBS_ID, "completed", 1);
    bad_obs_id["obs_id"] = json!(42);

    let mut negative_obs_id = job_detail(4, TEST_OBS_ID, "completed", 1);
    negative_obs_id["obs_id"] = json!(-1);

    let good = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "queued", 1);

    env.mock_get_jobs(vec![bad_obs_id, negative_obs_id, good]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    assert_eq!(jobs.0.len(), 1, "only the usable job should be returned");
    assert_eq!(jobs.0[0].job_id(), crate::test_config::TEST_ASVO_JOB_ID);
    assert_eq!(jobs.0[0].job_state, JobState::Queued);
}

/// A job that is not even the schema's `JobDetailResponse` (a Job ID of 0, or
/// no `obs_id`) fails the whole listing with a decode error: the schema
/// requires both, and the generated types check them.
#[test]
fn a_job_that_breaks_the_schema_fails_the_listing() {
    for break_it in [
        |job: &mut serde_json::Value| job["job_id"] = json!(0),
        |job: &mut serde_json::Value| {
            job.as_object_mut().expect("an object").remove("obs_id");
        },
    ] {
        let env = TestEnv::with_session();
        let mut broken = job_detail(1, TEST_OBS_ID, "completed", 1);
        break_it(&mut broken);
        env.mock_get_jobs(vec![broken]);

        let client = AsvoClient::new(client_config(&env)).expect("client should be created");
        let err = client
            .get_jobs(&JobsFilter::default())
            .expect_err("a job that breaks the schema must fail the listing");

        assert!(matches!(err, AsvoApiError::BadJson(_)), "{err:?}");
    }
}

/// Since schema 1.13 the job state is the schema's `JobState`, not free text:
/// a state that the schema does not list is a decode error of the whole
/// listing, not a skipped job. (The user chose this over skipping the job.)
#[test]
fn a_job_state_the_schema_does_not_list_fails_the_listing() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(1, TEST_OBS_ID, "queued", 1),
        job_detail(2, TEST_OBS_ID, "wibble", 1),
    ]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("an unlisted state must fail the listing");

    assert!(matches!(err, AsvoApiError::BadJson(_)), "{err:?}");
    assert!(err.to_string().contains("wibble"), "{err}");
}

/// The server may give no `job_type` (schema 1.13); the job has no type.
#[test]
fn a_job_without_a_type_has_none() {
    let env = TestEnv::with_session();
    let mut typeless = job_detail(1, TEST_OBS_ID, "queued", 1);
    typeless["job_type"] = serde_json::Value::Null;
    let mut missing = job_detail(2, TEST_OBS_ID, "queued", 1);
    missing
        .as_object_mut()
        .expect("a job is an object")
        .remove("job_type");
    env.mock_get_jobs(vec![typeless, missing]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("get_jobs should succeed");

    let types: Vec<_> = jobs.0.iter().map(|j| j.job_type).collect();
    assert_eq!(types, [None, None]);
}

// ---------------------------------------------------------------------------
// Submission and cancellation
// ---------------------------------------------------------------------------

#[cfg(feature = "bin")]
#[test]
fn a_visibility_job_posts_the_body_the_cli_built() {
    let env = TestEnv::with_session();
    let params = vis_params_from_cli(&["giant-squid", "submit-vis", TEST_OBS_ID]);
    let expected = serde_json::to_value(&params).expect("params should serialise");

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/create")
            .json_body(expected);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(777));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_download_vis_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 777);
}

#[cfg(feature = "bin")]
#[test]
fn a_metadata_job_posts_to_the_same_endpoint_with_a_meta_download_type() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-meta", TEST_OBS_ID])
        .expect("arguments should parse")
    {
        Args::SubmitMeta { download, .. } => {
            download.to_meta_params(crate::test_config::test_obs_id())
        }
        _ => panic!("expected submit-meta"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/create")
            .json_body_includes(r#"{ "download_type": "meta" }"#);
        then.status(200).json_body(job_submitted_response(778));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_download_meta_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 778);
}

#[cfg(feature = "bin")]
#[test]
fn a_metadata_job_method_always_sends_a_meta_download_type() {
    let env = TestEnv::with_session();
    // Start from visibility params: the metadata method must replace the
    // download type.
    let params = vis_params_from_cli(&["giant-squid", "submit-vis", TEST_OBS_ID]);

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/create")
            .json_body_includes(r#"{ "download_type": "meta" }"#);
        then.status(200).json_body(job_submitted_response(779));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_download_meta_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 779);
}

#[cfg(feature = "bin")]
#[test]
fn a_visibility_job_method_always_sends_a_vis_download_type() {
    let env = TestEnv::with_session();
    // Start from metadata params: the visibility method must replace the
    // download type.
    let params = match Args::try_parse_from(["giant-squid", "submit-meta", TEST_OBS_ID])
        .expect("arguments should parse")
    {
        Args::SubmitMeta { download, .. } => {
            download.to_meta_params(crate::test_config::test_obs_id())
        }
        _ => panic!("expected submit-meta"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/create")
            .json_body_includes(r#"{ "download_type": "vis" }"#);
        then.status(200).json_body(job_submitted_response(780));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_download_vis_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 780);
}

#[cfg(feature = "bin")]
#[test]
fn a_conversion_job_posts_to_the_conversion_endpoint() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-conv", TEST_OBS_ID])
        .expect("arguments should parse")
    {
        Args::SubmitConv { conv, .. } => conv.to_params(crate::test_config::test_obs_id()),
        _ => panic!("expected submit-conv"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/create")
            .json_body_includes(format!(r#"{{ "obs_id": {TEST_OBS_ID_I64} }}"#));
        then.status(200).json_body(job_submitted_response(779));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_conversion_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 779);
}

/// Every request the client makes for an end user goes to one of these
/// endpoints (the method and the path). The endpoints for the MWA ASVO's own
/// processors and scheduler (`/v2/job_staged`, `/v2/scheduler/*`,
/// `/v2/calibration_ready`) are not here, and must not be.
const END_USER_ENDPOINTS: [(&str, &str); 5] = [
    ("POST", "/api/v2/jobs/list"),
    ("POST", "/api/v2/jobs/create"),
    ("POST", "/api/v2/jobs/image"),
    ("DELETE", "/api/v2/jobs/12345"),
    ("POST", "/api/v2/auth/api_login"),
];

/// One request that the client made: the method, the path and the JSON body
/// (`null` when it had none).
type RecordedRequest = (String, String, serde_json::Value);

/// Make every call of the client that an end user can make (a login, a job
/// listing, the seven submissions and a cancellation) against a mock server,
/// with the default options, and return the requests in the order they were
/// made. The tests below check what the library sends and where.
fn record_every_end_user_call() -> Vec<RecordedRequest> {
    use crate::mwa_asvo::api::openapi::{
        ConversionJobParams, DownloadJobParams, DownloadType, ImagingJobFlow1Params,
        ImagingJobFlow2Params, VoltageJobParams,
    };
    use std::num::NonZeroU64;
    use std::sync::{Arc, Mutex};

    // No session on disk, so that the login is made and recorded too.
    let env = TestEnv::without_session();
    let seen: Arc<Mutex<Vec<(String, String, serde_json::Value)>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    env.server.mock(|when, then| {
        when.is_true(move |req| {
            let body = serde_json::from_slice(req.body_ref()).unwrap_or_default();
            recorder.lock().unwrap().push((
                req.method_str().to_string(),
                req.uri().path().to_string(),
                body,
            ));
            true
        });
        // One reply that every call can read: the login tokens, a job
        // submission, and an empty job list.
        then.status(200)
            .header("content-type", "application/json")
            .json_body({
                let mut reply = login_response();
                reply["job_id"] = json!(TEST_JOB_ID);
                reply["message"] = json!("ok");
                reply["status"] = json!("success");
                reply["jobs"] = json!([]);
                reply["total_count"] = json!(0);
                reply
            });
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let obs_id = TEST_OBS_ID_I64;
    let source_job_id = NonZeroU64::new(TEST_JOB_ID).unwrap();

    client.get_jobs(&JobsFilter::default()).expect("get_jobs");
    client
        .submit_download_vis_job(
            &DownloadJobParams::builder()
                .obs_id(obs_id)
                .download_type(DownloadType::Vis)
                .try_into()
                .expect("the vis body should build"),
        )
        .expect("vis");
    client
        .submit_conversion_job(
            &ConversionJobParams::builder()
                .obs_id(obs_id)
                .try_into()
                .expect("the conversion body should build"),
        )
        .expect("conversion");
    client
        .submit_imaging_job(
            &ImagingJobFlow1Params::builder()
                .obs_id(obs_id)
                .try_into()
                .expect("the imaging body should build"),
        )
        .expect("imaging");
    client
        .submit_image_from_job(
            &ImagingJobFlow2Params::builder()
                .source_job_id(source_job_id)
                .try_into()
                .expect("the image-from-job body should build"),
        )
        .expect("image from job");
    client
        .submit_voltage_job(
            &VoltageJobParams::builder()
                .obs_id(obs_id)
                .offset(0_i64)
                .duration(8_u64)
                .try_into()
                .expect("the voltage body should build"),
        )
        .expect("voltage");
    client
        .submit_download_beam_job(
            &DownloadJobParams::builder()
                .obs_id(obs_id)
                .download_type(DownloadType::Beam)
                .try_into()
                .expect("the beamformer body should build"),
        )
        .expect("beamformer");
    client
        .cancel_job(crate::test_config::TEST_ASVO_JOB_ID)
        .expect("cancel");

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 9, "one login and eight calls: {seen:?}");
    seen
}

/// The library calls only the endpoints of an end user.
#[test]
fn the_client_calls_only_end_user_endpoints() {
    for (method, path, _body) in record_every_end_user_call() {
        assert!(
            END_USER_ENDPOINTS.contains(&(method.as_str(), path.as_str())),
            "{method} {path} is not an end-user endpoint"
        );
    }
}

/// The schema that the request types were generated from.
const SCHEMA: &str = include_str!("../openapi-schema.json");

/// Whether the schema path `template` (with `{name}` parts, for example
/// `/api/v2/jobs/{job_id}`) matches the request path `path`.
fn path_matches(template: &str, path: &str) -> bool {
    let template: Vec<&str> = template.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    template.len() == path.len()
        && template
            .iter()
            .zip(&path)
            .all(|(t, p)| (t.starts_with('{') && t.ends_with('}')) || t == p)
}

/// Every endpoint that the client calls is a path of the schema (under
/// `API_PREFIX`, which the schema does not give), with the method the client
/// uses. The paths come from the `paths` key that
/// `tools/generate_openapi.sh` keeps in `openapi-schema.json`, so a path
/// that the API renames or removes fails here, not in a real request.
#[test]
fn every_endpoint_the_client_calls_is_a_schema_path() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    let paths = schema["paths"]
        .as_object()
        .expect("openapi-schema.json has no paths: run tools/generate_openapi.sh to regenerate it");
    let has = |method: &str, path: &str| {
        paths.iter().any(|(template, methods)| {
            path_matches(template, path)
                && methods
                    .as_array()
                    .is_some_and(|m| m.iter().any(|m| m.as_str() == Some(&method.to_lowercase())))
        })
    };

    let mut calls: Vec<(String, String)> = record_every_end_user_call()
        .into_iter()
        .map(|(method, path, _)| (method, path))
        .collect();
    // The refresh is not in the recorded calls (there is no session to
    // refresh), so it is checked from its constant.
    calls.push(("POST".to_string(), super::ENDPOINT_REFRESH.to_string()));
    for (method, path) in calls {
        let schema_path = path
            .strip_prefix(super::API_PREFIX)
            .unwrap_or_else(|| panic!("{path} is not under {}", super::API_PREFIX));
        assert!(
            has(&method, schema_path),
            "{method} {schema_path} is not a path of the schema"
        );
    }
}

/// The names of the schemas of the body of the operation `method` `path` (a
/// path of the schema, which has `{name}` parts), from the `operations` key
/// of the schema file. Empty when the operation has no body.
fn request_schemas(schema: &serde_json::Value, method: &str, path: &str) -> Vec<String> {
    let operations = schema["operations"]
        .as_object()
        .expect("openapi-schema.json has no operations: run tools/generate_openapi.sh");
    operations
        .iter()
        .find(|(key, _)| {
            key.split_once(' ')
                .is_some_and(|(m, template)| m == method && path_matches(template, path))
        })
        .unwrap_or_else(|| panic!("{method} {path} is not an operation of the schema"))
        .1["request"]
        .as_array()
        .expect("an operation has a request list")
        .iter()
        .map(|name| name.as_str().expect("a schema name").to_string())
        .collect()
}

/// Only parameters that the API defines are sent: every key of every body is
/// a property of that endpoint's schema. (In particular a conversion body has
/// no `flags`, which a listed conversion job's `job_params` shows but the
/// request schema does not have.) The bodies are the generated types, so this
/// is true by construction; the test keeps it true if someone ever builds a
/// body by hand.
#[test]
fn every_field_of_every_request_body_is_in_the_schema() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");

    for (method, path, body) in record_every_end_user_call() {
        // The schemas that the endpoint takes as its body: the `operations`
        // that `tools/generate_openapi.sh` keeps in the schema file. An
        // endpoint with more than one (`/v2/jobs/create` takes one of
        // several job types) takes any one of them.
        let schema_path = path
            .strip_prefix(super::API_PREFIX)
            .expect("under API_PREFIX");
        let names = request_schemas(&schema, &method, schema_path);
        if names.is_empty() {
            assert!(
                body.is_null(),
                "{method} {path} sent a body but has no schema: {body}"
            );
            continue;
        }
        let body = body.as_object().expect("a request body is a JSON object");

        assert!(!body.is_empty(), "{path} sent an empty body");
        assert!(!body.contains_key("flags"), "{path} sent flags");
        assert!(
            names.iter().any(|name| {
                let properties = schema["definitions"][name]["properties"]
                    .as_object()
                    .unwrap_or_else(|| panic!("the schema has no properties for {name}"));
                body.keys().all(|key| properties.contains_key(key))
            }),
            "{method} {path} sent {:?}, which are not all properties of one of {names:?}",
            body.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_cancellation_deletes_the_job_resource() {
    let env = TestEnv::with_session();
    let cancel = env.server.mock(|when, then| {
        when.method(DELETE)
            .path(format!("/api/v2/jobs/{TEST_JOB_ID}"));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(TEST_JOB_ID));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .cancel_job(crate::test_config::TEST_ASVO_JOB_ID)
        .expect("cancellation should succeed");

    assert_eq!(cancel.calls(), 1);
    assert_eq!(resp.job_id.get(), TEST_JOB_ID);
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

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .cancel_job(crate::test_config::job_id(999999))
        .expect_err("expected the cancellation to fail");

    assert!(is_api_error(&err, "JOB_NOT_FOUND"), "got {err:?}");
}

/// Imaging submissions return a `JobSubmittedResponse`, like every other
/// v2 submit endpoint, not a bare job ID.
#[cfg(feature = "bin")]
#[test]
fn an_imaging_job_returns_a_job_submitted_response() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from(["giant-squid", "submit-image", TEST_OBS_ID])
        .expect("arguments should parse")
    {
        Args::SubmitImage { image, .. } => image.to_params(crate::test_config::test_obs_id()),
        _ => panic!("expected submit-image"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/create");
        then.status(200).json_body(job_submitted_response(779));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_imaging_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 779);
}

#[cfg(feature = "bin")]
#[test]
fn an_image_from_job_submission_returns_a_job_submitted_response() {
    let env = TestEnv::with_session();
    let params = match Args::try_parse_from([
        "giant-squid",
        "submit-image-from-job",
        "--source-job-id",
        "12345",
    ])
    .expect("arguments should parse")
    {
        Args::SubmitImageFromJob { image, .. } => image.to_params(),
        _ => panic!("expected submit-image-from-job"),
    };

    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/image");
        then.status(200).json_body(job_submitted_response(780));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let resp = client
        .submit_image_from_job(&params)
        .expect("submission should succeed");

    assert_eq!(submit.calls(), 1);
    assert_eq!(resp.job_id.get(), 780);
}

/// A body the schema does not allow is refused by the library, so no
/// caller (the CLI, Python or another Rust program) can send it.
#[test]
fn an_out_of_range_imaging_job_is_not_sent() {
    let env = TestEnv::with_session();
    let mut params: crate::mwa_asvo::api::openapi::ImagingJobFlow1Params =
        crate::mwa_asvo::api::openapi::ImagingJobFlow1Params::builder()
            .obs_id(TEST_OBS_ID_I64)
            .try_into()
            .expect("defaults build");
    params.mgain = 1.5;
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/create");
        then.status(200).json_body(job_submitted_response(1));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .submit_imaging_job(&params)
        .expect_err("expected the submission to be refused");

    assert!(
        matches!(err, AsvoApiError::InvalidParameter { name: "mgain", .. }),
        "got {err:?}"
    );
    assert_eq!(submit.calls(), 0);
}

#[test]
fn an_out_of_range_image_from_job_is_not_sent() {
    let env = TestEnv::with_session();
    let mut params: crate::mwa_asvo::api::openapi::ImagingJobFlow2Params =
        crate::mwa_asvo::api::openapi::ImagingJobFlow2Params::builder()
            .obs_id(TEST_OBS_ID_I64)
            .source_job_id(std::num::NonZeroU64::new(12345).unwrap())
            .try_into()
            .expect("defaults build");
    params.pixel_scale = 5.0;
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/image");
        then.status(200).json_body(job_submitted_response(1));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .submit_image_from_job(&params)
        .expect_err("expected the submission to be refused");

    assert!(
        matches!(
            err,
            AsvoApiError::InvalidParameter {
                name: "pixel_scale",
                ..
            }
        ),
        "got {err:?}"
    );
    assert_eq!(submit.calls(), 0);
}

#[test]
fn an_out_of_range_conversion_job_is_not_sent() {
    let env = TestEnv::with_session();
    let mut params: crate::mwa_asvo::api::openapi::ConversionJobParams =
        crate::mwa_asvo::api::openapi::ConversionJobParams::builder()
            .obs_id(TEST_OBS_ID_I64)
            .try_into()
            .expect("defaults build");
    params.avg_freq_res = Some(5000.0);
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/create");
        then.status(200).json_body(job_submitted_response(1));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .submit_conversion_job(&params)
        .expect_err("expected the submission to be refused");

    assert!(
        matches!(
            err,
            AsvoApiError::InvalidParameter {
                name: "avg_freq_res",
                ..
            }
        ),
        "got {err:?}"
    );
    assert_eq!(submit.calls(), 0);
}

#[test]
fn an_out_of_range_voltage_job_is_not_sent() {
    let env = TestEnv::with_session();
    let params: crate::mwa_asvo::api::openapi::VoltageJobParams =
        crate::mwa_asvo::api::openapi::VoltageJobParams::builder()
            .obs_id(TEST_OBS_ID_I64)
            .offset(9999_i64)
            .duration(8_u64)
            .try_into()
            .expect("params build");
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/create");
        then.status(200).json_body(job_submitted_response(1));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .submit_voltage_job(&params)
        .expect_err("expected the submission to be refused");

    assert!(
        matches!(err, AsvoApiError::InvalidParameter { name: "offset", .. }),
        "got {err:?}"
    );
    assert_eq!(submit.calls(), 0);
}

/// A voltage body sends the channel bounds that the caller gave, and no
/// `channel_range`: the schema does not have it any more.
#[test]
fn a_voltage_body_sends_the_channels_and_no_channel_range() {
    for (from_channel, to_channel) in [(None, None), (Some(109), None), (None, Some(132))] {
        let env = TestEnv::with_session();
        let mut params = crate::mwa_asvo::api::job_params::voltage_params(
            crate::test_config::test_obs_id(),
            0,
            8,
        );
        params.from_channel = from_channel;
        params.to_channel = to_channel;
        let submit = env.server.mock(|when, then| {
            when.method(POST)
                .path("/api/v2/jobs/create")
                .is_true(move |req| {
                    let body: serde_json::Value =
                        serde_json::from_slice(req.body_ref()).unwrap_or_default();
                    body.get("channel_range").is_none()
                        && body.get("from_channel").and_then(|v| v.as_u64())
                            == from_channel.map(u64::from)
                        && body.get("to_channel").and_then(|v| v.as_u64())
                            == to_channel.map(u64::from)
                });
            then.status(200).json_body(job_submitted_response(1));
        });

        let client = AsvoClient::new(client_config(&env)).expect("client should be created");
        client.submit_voltage_job(&params).expect("submitted");

        assert_eq!(submit.calls(), 1, "{from_channel:?} {to_channel:?}");
    }
}

/// The MWA ASVO delivers to Acacia as a tar only: every submit method refuses
/// `delivery_format` `files` with `delivery` `acacia` before any request, and
/// sends the other combinations.
#[test]
fn acacia_delivery_with_files_is_refused_before_any_request() {
    use crate::mwa_asvo::api::job_params::{
        conversion_params, download_params, image_from_job_params, imaging_params,
    };
    use crate::mwa_asvo::api::openapi::{
        DeliveryDestination, DeliveryFormat, DownloadType, JobSubmittedResponse,
    };

    let env = TestEnv::with_session();
    let submit = env.server.mock(|when, then| {
        when.method(POST);
        then.status(200).json_body(job_submitted_response(1));
    });
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let obs_id = crate::test_config::test_obs_id();
    let source_job_id = crate::test_config::nonzero(TEST_JOB_ID);

    let refused = |result: Result<JobSubmittedResponse, AsvoApiError>| {
        let err = result.expect_err("acacia with files should be refused");
        assert!(
            matches!(
                err,
                AsvoApiError::InvalidParameter {
                    name: "delivery_format",
                    ..
                }
            ),
            "got {err:?}"
        );
        assert!(err.to_string().contains("tar"), "{err}");
    };

    // The schema's defaults are acacia and tar, so each body is acacia with
    // files once the format is set.
    let mut vis = download_params(obs_id, DownloadType::Vis);
    vis.delivery_format = DeliveryFormat::Files;
    refused(client.submit_download_vis_job(&vis));
    refused(client.submit_download_meta_job(&vis));
    let mut conversion = conversion_params(obs_id);
    conversion.delivery_format = DeliveryFormat::Files;
    refused(client.submit_conversion_job(&conversion));
    let mut imaging = imaging_params(obs_id);
    imaging.delivery_format = DeliveryFormat::Files;
    refused(client.submit_imaging_job(&imaging));
    let mut image_from_job = image_from_job_params(source_job_id);
    image_from_job.delivery_format = DeliveryFormat::Files;
    refused(client.submit_image_from_job(&image_from_job));
    let mut beamformer = download_params(obs_id, DownloadType::Beam);
    beamformer.delivery_format = DeliveryFormat::Files;
    refused(client.submit_download_beam_job(&beamformer));
    assert_eq!(submit.calls(), 0, "nothing should be sent");

    // The other combinations are sent: acacia with tar, and the other
    // deliveries with either format.
    let mut sent = 0;
    for (delivery, delivery_format) in [
        (DeliveryDestination::Acacia, DeliveryFormat::Tar),
        (DeliveryDestination::Scratch, DeliveryFormat::Files),
        (DeliveryDestination::Scratch, DeliveryFormat::Tar),
        (DeliveryDestination::Dug, DeliveryFormat::Files),
        (DeliveryDestination::Dug, DeliveryFormat::Tar),
    ] {
        vis.delivery = delivery;
        vis.delivery_format = delivery_format;
        client
            .submit_download_vis_job(&vis)
            .expect("a valid delivery");
        conversion.delivery = delivery;
        conversion.delivery_format = delivery_format;
        client
            .submit_conversion_job(&conversion)
            .expect("a valid delivery");
        imaging.delivery = delivery;
        imaging.delivery_format = delivery_format;
        client
            .submit_imaging_job(&imaging)
            .expect("a valid delivery");
        image_from_job.delivery = delivery;
        image_from_job.delivery_format = delivery_format;
        client
            .submit_image_from_job(&image_from_job)
            .expect("a valid delivery");
        beamformer.delivery = delivery;
        beamformer.delivery_format = delivery_format;
        client
            .submit_download_beam_job(&beamformer)
            .expect("a valid delivery");
        sent += 5;
    }
    assert_eq!(submit.calls(), sent);
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
const RECORDED_JOB_ID: AsvoJobId = crate::test_config::job_id(30000517);
const RECORDED_OBS_ID: u64 = 1115977528;
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

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    // The recording was made with --days 30, and the recorded request is
    // the matching criteria, so the same argument is required here.
    let jobs = client
        .get_jobs(&JobsFilter {
            days: Some(crate::test_config::nonzero(30)),
            ..JobsFilter::default()
        })
        .expect("the recorded listing should be served");

    assert_eq!(jobs.0.len(), 1);
    let job = &jobs.0[0];
    assert_eq!(job.job_id(), RECORDED_JOB_ID);
    assert_eq!(job.obs_id().get(), RECORDED_OBS_ID);
    assert_eq!(job.job_type, Some(job_type("metadata")));
    assert_eq!(job.job_state, JobState::Completed);
    assert!(job.completed.is_some(), "completed should be parsed");
}

/// The recording pins the schema's `JobProduct` against a real payload.
/// (Before schema 1.11, `product` was a free-form object.)
#[test]
fn a_recorded_jobs_product_becomes_a_file_list() {
    let env = TestEnv::with_session();
    env.server.playback(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("login_and_get_jobs.yaml"),
    );

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter {
            days: Some(crate::test_config::nonzero(30)),
            ..JobsFilter::default()
        })
        .expect("the recorded listing should be served");

    let files = &jobs.0[0]
        .product
        .as_ref()
        .expect("a completed job should carry its files")
        .files;
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.type_, FileType::Acacia);
    assert_eq!(file.size_bytes(), RECORDED_SIZE);
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

#[cfg(feature = "bin")]
const TARGET_ENV: &str = "MWA_ASVO_RECORD_TARGET";

#[cfg(feature = "bin")]
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

    // Send giant-squid's own client through the recording server. The
    // rest of the config (API key, HOME) comes from the environment, as it
    // does for the CLI.
    let mut config = client_config_from_env().expect("MWA_ASVO_API_KEY must be set");
    config.host = server.base_url();

    let client = AsvoClient::new(config).expect("could not authenticate with the target server");
    let jobs = client
        .get_jobs(&JobsFilter {
            days: Some(crate::test_config::nonzero(30)),
            ..JobsFilter::default()
        })
        .expect("could not list jobs on the target server");
    println!("recorded a login and a listing of {} jobs", jobs.0.len());

    let path = recording.save("login_and_get_jobs").expect("save failed");
    println!("raw recording: {}", path.display());
    println!(
        "scrub it before committing - see the section notes in src/mwa_asvo/api/client/tests.rs"
    );
}

/// Since schema v1.11 a product file has a `format`, which is kept.
#[test]
fn a_product_file_keeps_its_format() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    detail["product"] = json!({
        "files": [{ "type": "acacia", "url": "https://example.org/x.tar", "size": 3,
                    "sha1": "0000000000000000000000000000000000000000", "format": "tar" }]
    });
    env.mock_get_jobs(vec![detail]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("the listing should succeed");

    let files = &jobs.0[0].product.as_ref().expect("the job has files").files;
    assert_eq!(files[0].format.as_deref(), Some("tar"));
    assert_eq!(files[0].size, 3);
}

/// A product with no `files` means no files: it does not fail the listing.
#[test]
fn a_product_without_files_does_not_fail_the_listing() {
    let env = TestEnv::with_session();
    let mut empty = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    empty["product"] = json!({});
    let other = job_detail(TEST_JOB_ID as i64 + 1, TEST_OBS_ID, "queued", 1);
    env.mock_get_jobs(vec![empty, other]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("the listing should succeed");

    assert_eq!(jobs.0.len(), 2);
    assert!(jobs.0[0].product.is_none());
}

// ---------------------------------------------------------------------------
// get_jobs filters, the full job detail, and API error details
// ---------------------------------------------------------------------------

/// Every filter reaches the request body under its OpenAPI name, with the
/// API's own values (a `Ready` job is `completed`; a type is its number). The
/// dates are in their own test, because `days` and the dates are exclusive.
#[test]
fn get_jobs_sends_every_filter_to_the_server() {
    let env = TestEnv::with_session();
    let filtered = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(
                r#"{ "days": 7, "job_state": "completed", "job_type": "imaging",
                 "sort_by": "created" }"#,
            );
        then.status(200)
            .json_body(json!({ "jobs": [], "total_count": 0 }));
    });

    let filter = JobsFilter {
        days: Some(crate::test_config::nonzero(7)),
        job_states: vec![JobState::Completed],
        job_types: vec![job_type("imaging")],
        sort_by: Some(crate::mwa_asvo::SortBy::Created),
        ..JobsFilter::default()
    };
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .get_jobs(&filter)
        .expect("the listing should succeed");

    assert_eq!(filtered.calls(), 1);
}

/// With no filter, nothing is filtered: `days` and the order are the
/// schema's defaults (not `null`, which would be a request of its own), and
/// no other filter is sent.
#[test]
fn get_jobs_with_no_filter_uses_the_schema_defaults() {
    let env = TestEnv::with_session();
    let schema_default_days = JobsByUserRequest::default()
        .days
        .expect("the schema has a default for days")
        .get();
    let unfiltered = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(json!({ "days": schema_default_days, "sort_by": "id" }).to_string())
            .is_true(|req| {
                let body: serde_json::Value =
                    serde_json::from_slice(req.body().as_ref()).unwrap_or_default();
                ["job_state", "job_type", "date_from", "date_to"]
                    .iter()
                    .all(|key| body.get(key).is_none())
            });
        then.status(200)
            .json_body(json!({ "jobs": [], "total_count": 0 }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .get_jobs(&JobsFilter::default())
        .expect("the listing should succeed");

    assert_eq!(unfiltered.calls(), 1);
}

/// The jobs are selected by `days` or by the dates, not by both: a filter with
/// `days` and a date is refused before any request, by `get_jobs` and by
/// `JobsFilter::validate`.
#[test]
fn days_with_a_date_is_refused_before_any_request() {
    let env = TestEnv::with_session();
    let listing = env.mock_get_jobs(vec![]);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let time: jiff::Timestamp = "2026-09-01T00:00:00Z".parse().expect("a valid time");

    for (date_from, date_to) in [
        (Some(time), None),
        (None, Some(time)),
        (Some(time), Some(time)),
    ] {
        let filter = JobsFilter {
            days: Some(crate::test_config::nonzero(7)),
            date_from,
            date_to,
            ..JobsFilter::default()
        };

        for err in [
            filter.validate().expect_err("the filter is refused"),
            client.get_jobs(&filter).expect_err("get_jobs is refused"),
        ] {
            assert!(
                matches!(err, AsvoApiError::InvalidParameter { name: "days", .. }),
                "got {err:?}"
            );
            assert!(err.to_string().contains("date_from"), "{err}");
        }
    }
    assert_eq!(listing.calls(), 0);
}

/// With a date and no `days`, the request has `"days": null` and the dates,
/// so that the default of `days` does not limit them. With `days` and no
/// date, the request has `days` and no date.
#[test]
fn a_date_sends_null_days_and_days_alone_sends_no_date() {
    let env = TestEnv::with_session();
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let empty = json!({ "jobs": [], "total_count": 0 });

    let mut dated = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list").json_body_includes(
            json!({ "days": null, "date_from": "2026-09-01T00:00:00Z", "date_to": "2026-09-30T00:00:00Z" })
                .to_string(),
        );
        then.status(200).json_body(empty.clone());
    });
    client
        .get_jobs(&JobsFilter {
            date_from: Some("2026-09-01T00:00:00Z".parse().expect("a valid time")),
            date_to: Some("2026-09-30T00:00:00Z".parse().expect("a valid time")),
            ..JobsFilter::default()
        })
        .expect("the listing with dates should succeed");
    assert_eq!(dated.calls(), 1);
    dated.delete();

    // One date is enough to make `days` null.
    let mut one_date = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(
                json!({ "days": null, "date_to": "2026-09-30T00:00:00Z" }).to_string(),
            );
        then.status(200).json_body(empty.clone());
    });
    client
        .get_jobs(&JobsFilter {
            date_to: Some("2026-09-30T00:00:00Z".parse().expect("a valid time")),
            ..JobsFilter::default()
        })
        .expect("the listing with one date should succeed");
    assert_eq!(one_date.calls(), 1);
    one_date.delete();

    let by_days = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(json!({ "days": 7 }).to_string())
            .is_true(|req| {
                let body: serde_json::Value =
                    serde_json::from_slice(req.body().as_ref()).unwrap_or_default();
                ["date_from", "date_to"]
                    .iter()
                    .all(|key| body.get(key).is_none())
            });
        then.status(200).json_body(empty.clone());
    });
    client
        .get_jobs(&JobsFilter {
            days: Some(crate::test_config::nonzero(7)),
            ..JobsFilter::default()
        })
        .expect("the listing by days should succeed");
    assert_eq!(by_days.calls(), 1);
}

/// A `days` above the schema's 1 to 30 is refused before any request, by
/// `get_jobs` (and `JobsFilter::validate`); the ends are accepted.
#[test]
fn days_outside_the_schema_limits_are_refused_before_any_request() {
    let env = TestEnv::with_session();
    let listing = env.mock_get_jobs(vec![]);
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");

    // The type refuses 0; the library refuses what is above 30.
    for days in [31, u64::MAX].map(crate::test_config::nonzero) {
        let err = client
            .get_jobs(&JobsFilter {
                days: Some(days),
                ..JobsFilter::default()
            })
            .expect_err("days should be refused");
        assert!(
            matches!(err, AsvoApiError::InvalidParameter { name: "days", .. }),
            "got {err:?}"
        );
        assert!(
            err.to_string().contains("between 1 and 30"),
            "the message should give the limits: {err}"
        );

        assert!(matches!(
            JobsFilter {
                days: Some(days),
                ..JobsFilter::default()
            }
            .validate(),
            Err(AsvoApiError::InvalidParameter { name: "days", .. })
        ));
    }
    assert_eq!(listing.calls(), 0);

    for days in [1, 30].map(crate::test_config::nonzero) {
        client
            .get_jobs(&JobsFilter {
                days: Some(days),
                ..JobsFilter::default()
            })
            .expect("the end of the range should be accepted");
    }
    assert_eq!(listing.calls(), 2);
}

/// Every field of the job detail reaches `AsvoJob`.
#[test]
fn a_listed_job_has_every_field_of_the_job_detail() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "error", 1);
    detail["error_code"] = json!(7);
    detail["error_text"] = json!("it failed");
    detail["started"] = json!("2026-09-08T05:50:00");
    detail["modified"] = json!("2026-09-08T05:55:00Z");
    env.mock_get_jobs(vec![detail]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("the listing should succeed");
    let job = &jobs.0[0];

    let utc = |time: &str| time.parse::<jiff::Timestamp>().expect("a valid time");
    assert_eq!(job.created, utc("2026-09-08T05:41:54.757232Z"));
    assert_eq!(job.started, Some(utc("2026-09-08T05:50:00Z")));
    assert_eq!(job.modified, Some(utc("2026-09-08T05:55:00Z")));
    assert_eq!(job.error_code, Some(7));
    assert_eq!(job.error_text.as_deref(), Some("it failed"));
    assert_eq!(job.job_state, JobState::Error);
    assert_eq!(job.user_id, TEST_USER_ID);
    assert_eq!(job.first_name, "Test");
    assert_eq!(job.last_name, "User");
    assert_eq!(job.job_params["delivery"], "acacia");
}

/// An API error keeps its field errors and request ID, and shows them in
/// its message.
#[test]
fn an_api_error_keeps_its_field_errors_and_request_id() {
    let env = TestEnv::with_session();
    let mut body = error_response("VALIDATION_ERROR", "Invalid parameters");
    body["field_errors"] = json!([
        { "field": "mgain", "message": "must be at most 1" },
        { "field": "robust", "message": "must be at least -2" }
    ]);
    body["request_id"] = json!("req-1234");
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(422).json_body(body);
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("expected the listing to fail");

    match &err {
        AsvoApiError::ApiError { response, .. } => {
            let fields: Vec<&str> = response
                .field_errors
                .iter()
                .flatten()
                .map(|e| e.field.as_str())
                .collect();
            assert_eq!(fields, ["mgain", "robust"]);
            assert_eq!(response.request_id.as_deref(), Some("req-1234"));
        }
        other => panic!("expected ApiError, got {other:?}"),
    }
    assert_eq!(
        err.to_string(),
        "MWA ASVO returned an error (VALIDATION_ERROR): Invalid parameters\n  Detail: detail from \
         the mock server\n  Suggestion: try something else\n  mgain: must be at most 1\n  robust: \
         must be at least -2\n  (request ID: req-1234)"
    );
}

/// An error that the server gives with no detail, suggestion, field errors or
/// request ID has the plain message.
#[test]
fn an_api_error_without_details_has_the_plain_message() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(400)
            .json_body(json!({ "error_code": "JOB_INVALID_STATE", "message": "Job is not ready" }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("expected the listing to fail");

    assert_eq!(
        err.to_string(),
        "MWA ASVO returned an error (JOB_INVALID_STATE): Job is not ready"
    );
}

/// The server's `detail` and `suggestion` are in the message, as the server
/// wrote them, each on its own line, so a user sees them in the `giant-squid`
/// command and in the Python exception.
#[test]
fn an_api_error_shows_the_servers_detail_and_suggestion() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(400)
            .json_body(error_response("JOB_INVALID_STATE", "Job is not ready"));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("expected the listing to fail");

    assert_eq!(
        err.to_string(),
        "MWA ASVO returned an error (JOB_INVALID_STATE): Job is not ready\n  Detail: detail from \
         the mock server\n  Suggestion: try something else"
    );
}

/// Only the part the server gave is shown.
#[test]
fn an_api_error_with_only_a_suggestion_shows_only_the_suggestion() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list");
        then.status(400).json_body(json!({
            "error_code": "JOB_INVALID_STATE",
            "message": "Job is not ready",
            "suggestion": "wait a minute",
        }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&JobsFilter::default())
        .expect_err("expected the listing to fail");

    assert_eq!(
        err.to_string(),
        "MWA ASVO returned an error (JOB_INVALID_STATE): Job is not ready\n  Suggestion: wait a minute"
    );
}

// ---------------------------------------------------------------------------
// get_jobs with lists: the listing that the CLI's list and wait use
// ---------------------------------------------------------------------------

/// Three jobs: a ready conversion, a queued imaging job and a cancelled
/// visibility download. (The API has no expired state since schema 1.13.)
fn three_jobs() -> Vec<serde_json::Value> {
    vec![
        job_detail(1, TEST_OBS_ID, "completed", 0),
        job_detail(2, TEST_OBS_ID, "queued", 6),
        job_detail(3, "1090008640", "cancelled", 1),
    ]
}

/// One state and one type go to the server; the listing is then filtered
/// on the client too, so the result is right whatever the server does.
#[test]
fn get_jobs_sends_a_single_state_and_type_to_the_server() {
    let env = TestEnv::with_session();
    let listing = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/jobs/list")
            .json_body_includes(r#"{ "job_state": "queued", "job_type": "imaging" }"#);
        then.status(200)
            .json_body(json!({ "jobs": three_jobs(), "total_count": 3 }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter {
            job_states: vec![JobState::Queued],
            job_types: vec![job_type("imaging")],
            ..JobsFilter::default()
        })
        .expect("the listing should succeed");

    assert_eq!(listing.calls(), 1);
    let ids: Vec<AsvoJobId> = jobs.0.iter().map(|j| j.job_id()).collect();
    assert_eq!(ids, [crate::test_config::job_id(2)]);
}

/// Several states are not a server filter; they are applied to the result.
#[test]
fn get_jobs_filters_several_states_on_the_client() {
    let env = TestEnv::with_session();
    let listing = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/jobs/list").is_true(|req| {
            let body: serde_json::Value =
                serde_json::from_slice(req.body().as_ref()).unwrap_or_default();
            body.get("job_state").is_none() && body.get("job_type").is_none()
        });
        then.status(200)
            .json_body(json!({ "jobs": three_jobs(), "total_count": 3 }));
    });

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter {
            job_states: vec![JobState::Completed, JobState::Queued],
            ..JobsFilter::default()
        })
        .expect("the listing should succeed");

    assert_eq!(listing.calls(), 1);
    let ids: Vec<AsvoJobId> = jobs.0.iter().map(|j| j.job_id()).collect();
    assert_eq!(
        ids,
        [crate::test_config::job_id(1), crate::test_config::job_id(2)]
    );
}

/// Job IDs and obsids together are refused before any request.
#[test]
fn get_jobs_refuses_job_ids_and_obs_ids_together() {
    let env = TestEnv::with_session();
    let listing = env.mock_get_jobs(vec![]);
    let filter = JobsFilter {
        job_ids: vec![crate::test_config::job_id(1)],
        obs_ids: vec![crate::ObsId::validate(1065880128).expect("a valid obsid")],
        ..JobsFilter::default()
    };

    assert!(matches!(
        filter.validate(),
        Err(AsvoApiError::InvalidParameter {
            name: "job_ids",
            ..
        })
    ));
    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .get_jobs(&filter)
        .expect_err("the filter should be refused");

    assert!(err.to_string().contains("can't specify both"), "{err}");
    assert_eq!(listing.calls(), 0);
}

/// The server takes one state and one type: a list with exactly one entry
/// gives the server's filter, and a list of none or of several gives none.
#[test]
fn a_list_of_one_entry_is_the_servers_filter() {
    assert_eq!(super::single(&[JobState::Queued]), Some(JobState::Queued));
    assert_eq!(
        super::single(&[job_type("conversion")]),
        Some(job_type("conversion"))
    );
    assert_eq!(super::single::<JobState>(&[]), None);
    assert_eq!(
        super::single(&[JobState::Queued, JobState::Completed]),
        None
    );
}

/// A time with a zone is left as it is, whatever the zone is written like (a
/// lowercase `z` is as valid as `Z` in RFC 3339), and a time with none gets
/// `Z`. A lowercase `z` once got a second `Z`, which failed the whole listing.
#[test]
fn a_time_with_any_zone_is_left_alone_and_a_time_without_one_gets_z() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    detail["created"] = json!("2026-09-08T05:41:54z");
    detail["started"] = json!("2026-09-08T05:50:00+08:00");
    detail["modified"] = json!("2026-09-08T05:55:00");
    env.mock_get_jobs(vec![detail]);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let jobs = client
        .get_jobs(&JobsFilter::default())
        .expect("the listing should succeed");
    let job = &jobs.0[0];

    let utc = |time: &str| time.parse::<jiff::Timestamp>().expect("a valid time");
    assert_eq!(job.created, utc("2026-09-08T05:41:54Z"));
    assert_eq!(job.started, Some(utc("2026-09-07T21:50:00Z")));
    assert_eq!(job.modified, Some(utc("2026-09-08T05:55:00Z")));
}
