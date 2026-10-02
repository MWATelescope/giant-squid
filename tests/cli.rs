// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests: the built `giant-squid` binary is run as a subprocess
//! against a local mock server.
//!
//! These cover the parts of the CLI that live in `main` rather than in the
//! library - `--dry-run`, environment-variable defaults, filtering, JSON
//! output, exit codes - which the in-process tests cannot reach. Nothing is
//! submitted to a real MWA ASVO. See docs/TESTING.md.

mod common;

use common::*;
use httpmock::{prelude::*, Mock};

/// A mock that matches anything, used to prove no request was made.
fn catch_all(env: &CliEnv) -> Mock<'_> {
    env.server.mock(|when, then| {
        when.any_request();
        then.status(500).body("no request should have reached here");
    })
}

// ---------------------------------------------------------------------------
// Basics
// ---------------------------------------------------------------------------

#[test]
fn version_and_help_need_no_server() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.arg("--version");
    let version = run(cmd);
    assert!(version.success, "output: {}", version.combined());
    assert!(
        version.stdout.contains(env!("CARGO_PKG_VERSION")),
        "stdout: {}",
        version.stdout
    );

    let mut cmd = env.command();
    cmd.arg("--help");
    let help = run(cmd);
    assert!(help.success, "output: {}", help.combined());
    assert!(
        help.stdout.contains("submit-vis"),
        "stdout: {}",
        help.stdout
    );

    assert_eq!(requests.calls(), 0);
}

#[test]
fn an_unknown_command_exits_with_a_failure() {
    let env = CliEnv::with_session();
    let mut cmd = env.command();
    cmd.args(["submit-nothing", TEST_OBS_ID]);

    let result = run(cmd);

    assert!(!result.success);
    assert!(
        result.combined().contains("unrecognized subcommand"),
        "output: {}",
        result.combined()
    );
}

// ---------------------------------------------------------------------------
// --dry-run makes no requests
// ---------------------------------------------------------------------------

#[test]
fn a_dry_run_submission_contacts_no_server() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["submit-vis", "--dry-run", TEST_OBS_ID]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(requests.calls(), 0, "a dry run must not call the API");
    assert!(
        result.combined().contains("Would have submitted"),
        "output: {}",
        result.combined()
    );
}

/// A dry run prints the endpoint and the resolved JSON body, so it shows
/// what would actually be sent rather than echoing the arguments back.
#[test]
fn a_dry_run_prints_the_endpoint_and_the_request_body() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args([
        "submit-conv",
        "--dry-run",
        "--delivery",
        "scratch",
        "--avg-freq-res",
        "40",
        TEST_OBS_ID,
    ]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(requests.calls(), 0);
    let output = result.combined();
    for expected in [
        "/api/v2/conversion_job",
        "\"obs_id\": 1065880128",
        "\"delivery\": \"scratch\"",
        "\"avg_freq_res\": 40",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected} in the dry-run output: {output}"
        );
    }
}

#[test]
fn a_dry_run_prints_one_body_per_obs_id() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["submit-vis", "--dry-run", "1061311664", "1061311784"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(requests.calls(), 0);
    let output = result.combined();
    assert!(
        output.contains("\"obs_id\": 1061311664"),
        "output: {output}"
    );
    assert!(
        output.contains("\"obs_id\": 1061311784"),
        "output: {output}"
    );
}

#[test]
fn a_dry_run_cancellation_names_the_job_resource() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["cancel", "--dry-run", "12345"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(requests.calls(), 0);
    assert!(
        result.combined().contains("/api/v2/jobs/12345"),
        "output: {}",
        result.combined()
    );
}

#[test]
fn a_dry_run_cancellation_contacts_no_server() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["cancel", "--dry-run", "12345"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(requests.calls(), 0);
    assert!(
        result.combined().contains("Would have cancelled"),
        "output: {}",
        result.combined()
    );
}

// ---------------------------------------------------------------------------
// Submission
// ---------------------------------------------------------------------------

#[test]
fn a_submission_reaches_the_server_and_reports_the_job_id() {
    let env = CliEnv::with_session();
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.args(["submit-vis", TEST_OBS_ID]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(submit.calls(), 1);
    assert!(
        result.combined().contains("4321"),
        "the job ID should be reported: {}",
        result.combined()
    );
}

#[test]
fn several_obs_ids_are_submitted_one_request_each() {
    let env = CliEnv::with_session();
    let submit = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.args(["submit-vis", "1061311664", "1061311784", "1061312032"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(submit.calls(), 3);
}

/// `--json` prints the server's `JobSubmittedResponse` for each submitted
/// job, one compact object per line, so scripts need not parse log text.
#[test]
fn json_prints_one_submitted_response_per_obs_id() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.args(["submit-vis", "--json", "1061311664", "1061311784"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    let responses: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .map(|l| serde_json::from_str(l.trim()).expect("each JSON line should parse"))
        .collect();
    assert_eq!(responses.len(), 2, "stdout: {}", result.stdout);
    for resp in &responses {
        assert_eq!(resp["job_id"], 4321);
        assert_eq!(resp["status"], "success");
    }
}

#[test]
fn json_works_for_image_from_job_submissions() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/image_from_job");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4322));
    });

    let mut cmd = env.command();
    cmd.args([
        "submit-image-from-job",
        "--json",
        "--source-job-id",
        "12345",
        TEST_OBS_ID,
    ]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(result.stdout_json()["job_id"], 4322);
}

/// The delivery defaults can come from the environment, which clap reads at
/// parse time - the one part of the CLI matrix the in-process tests cannot
/// exercise, since the value must be set before the process starts.
#[test]
fn the_delivery_environment_variables_supply_the_defaults() {
    let env = CliEnv::with_session();
    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(r#"{ "delivery": "scratch", "delivery_format": "tar" }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.env("GIANT_SQUID_DELIVERY", "scratch")
        .env("GIANT_SQUID_DELIVERY_FORMAT", "tar")
        .args(["submit-vis", TEST_OBS_ID]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(submit.calls(), 1, "the env defaults should reach the body");
}

#[test]
fn a_command_line_flag_beats_the_environment() {
    let env = CliEnv::with_session();
    let submit = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(r#"{ "delivery": "acacia" }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.env("GIANT_SQUID_DELIVERY", "scratch").args([
        "submit-vis",
        "--delivery",
        "acacia",
        TEST_OBS_ID,
    ]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(submit.calls(), 1);
}

#[test]
fn a_job_id_where_an_obs_id_is_required_is_rejected_before_any_request() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["submit-vis", "12345"]);
    let result = run(cmd);

    assert!(!result.success);
    assert_eq!(requests.calls(), 0);
    assert!(
        result.combined().contains("Expected only obsids"),
        "output: {}",
        result.combined()
    );
}

#[test]
fn submitting_with_no_obs_ids_is_rejected() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.arg("submit-vis");
    let result = run(cmd);

    assert!(!result.success);
    assert_eq!(requests.calls(), 0);
    assert!(
        result.combined().contains("No obsids specified"),
        "output: {}",
        result.combined()
    );
}

/// A failing obsid must not hide the ones after it: every obsid is
/// attempted, each failure is reported, and the run fails at the end.
#[test]
fn one_failing_obs_id_does_not_stop_the_others() {
    let env = CliEnv::with_session();
    let rejected = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(r#"{ "obs_id": 1115977528 }"#);
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response(
                "JOB_ALREADY_SUBMITTED",
                "This job has already been submitted",
            ));
    });
    let accepted = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(r#"{ "obs_id": 1061311664 }"#);
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(4321));
    });

    let mut cmd = env.command();
    cmd.args(["submit-meta", "1115977528", "1061311664"]);
    let result = run(cmd);

    let output = result.combined();
    assert_eq!(rejected.calls(), 1);
    assert_eq!(
        accepted.calls(),
        1,
        "the second obsid should still be attempted: {output}"
    );
    assert!(!result.success, "the run should fail overall: {output}");
    assert!(output.contains("JOB_ALREADY_SUBMITTED"), "output: {output}");
    assert!(
        output.contains("Submitted 1 of 2 obsids"),
        "the summary should count both: {output}"
    );
}

#[test]
fn every_failure_is_listed_at_the_end() {
    let env = CliEnv::with_session();
    let rejected = env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("OBSID_NOT_FOUND", "No such observation"));
    });

    let mut cmd = env.command();
    cmd.args(["submit-meta", "1115977528", "1061311664", "1061311784"]);
    let result = run(cmd);

    let output = result.combined();
    assert_eq!(rejected.calls(), 3, "all three should be attempted");
    assert!(!result.success);
    for obs_id in ["1115977528", "1061311664", "1061311784"] {
        assert!(
            output.contains(obs_id),
            "{obs_id} should appear in the failure report: {output}"
        );
    }
    assert!(output.contains("3 of 3 obsids failed"), "output: {output}");
}

#[test]
fn a_server_error_on_submission_fails_the_run() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("OBSID_NOT_FOUND", "No such observation"));
    });

    let mut cmd = env.command();
    cmd.args(["submit-vis", TEST_OBS_ID]);
    let result = run(cmd);

    assert!(!result.success, "the run should fail");
    assert!(
        result.combined().contains("No such observation"),
        "output: {}",
        result.combined()
    );
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[test]
fn list_json_prints_the_jobs_keyed_by_job_id() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);

    let mut cmd = env.command();
    cmd.args(["list", "--json"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    let parsed = result.stdout_json();
    let jobs = parsed.as_object().expect("a map keyed by job ID");
    assert_eq!(jobs.len(), 1);
    assert_eq!(parsed["12345"]["job_id"], 12345);
}

/// `--legacy-json` prints the old keys, for one release, and says on
/// stderr that it is deprecated.
#[test]
fn list_legacy_json_prints_the_old_keys_and_a_warning() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);

    let mut cmd = env.command();
    cmd.args(["list", "--legacy-json"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    let parsed = result.stdout_json();
    assert_eq!(parsed["12345"]["jobId"], 12345);
    assert_eq!(parsed["12345"]["obsid"], 1065880128_u64);
    assert!(parsed["12345"].get("job_id").is_none());
    // On stderr, so that stdout is only the JSON a script reads.
    assert!(
        !result.stdout.contains("deprecated"),
        "stdout: {}",
        result.stdout
    );
    assert!(
        result.stderr.contains("--legacy-json is deprecated"),
        "stderr: {}\nstdout: {}",
        result.stderr,
        result.stdout
    );
}

/// Log records go to stderr, so that stdout is only the output a script
/// reads (for example `giant-squid list --json | jq`).
#[test]
fn list_json_puts_only_the_json_on_stdout() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);

    let mut cmd = env.command();
    cmd.args(["list", "--json"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    // The whole of stdout is one JSON document, with no log lines.
    let parsed: serde_json::Value = serde_json::from_str(result.stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not only JSON ({e}): {}", result.stdout));
    assert_eq!(parsed["12345"]["job_id"], 12345);
    // The mock server is not the default host, so the client always logs
    // a warning. It is on stderr.
    assert!(
        result.stderr.contains("non-default host"),
        "stderr: {}",
        result.stderr
    );
}

#[test]
fn list_prints_the_jobs_as_a_table() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);

    let mut cmd = env.command();
    cmd.args(["list", "--no-colour"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert!(
        result.stdout.contains("Job ID") && result.stdout.contains("Job State"),
        "expected the table header: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("12345") && result.stdout.contains(TEST_OBS_ID),
        "expected the job in the table: {}",
        result.stdout
    );
}

#[test]
fn list_reports_when_there_are_no_jobs() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![]);

    let mut cmd = env.command();
    cmd.arg("list");
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert!(
        result.stdout.contains("You have no jobs."),
        "output: {}",
        result.combined()
    );
}

#[test]
fn wait_returns_when_the_job_is_ready() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);

    let mut cmd = env.command();
    cmd.args(["wait", "--json", "12345"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    let parsed = result.stdout_json();
    assert_eq!(parsed["12345"]["job_id"], 12345);
}

#[test]
fn wait_fails_when_the_job_has_an_error() {
    let env = CliEnv::with_session();
    let mut job = job_detail(12345, TEST_OBS_ID, "error", 1);
    job["error_text"] = serde_json::json!("the conversion failed");
    env.mock_get_jobs(vec![job]);

    let mut cmd = env.command();
    cmd.args(["wait", "12345"]);
    let result = run(cmd);

    assert!(!result.success, "output: {}", result.combined());
    assert!(
        result.combined().contains(
            "MWA ASVO job ID 12345 (obsid: 1065880128) has an error: the conversion failed"
        ),
        "output: {}",
        result.combined()
    );
}

/// The error code of a failed job is in the error that `wait` reports.
#[test]
fn wait_shows_the_error_code_of_a_failed_job() {
    let env = CliEnv::with_session();
    let mut job = job_detail(12345, TEST_OBS_ID, "error", 1);
    job["error_code"] = serde_json::json!(7);
    job["error_text"] = serde_json::json!("the conversion failed");
    env.mock_get_jobs(vec![job]);

    let mut cmd = env.command();
    cmd.args(["wait", "12345"]);
    let result = run(cmd);

    assert!(!result.success, "output: {}", result.combined());
    assert!(
        result.combined().contains(
            "MWA ASVO job ID 12345 (obsid: 1065880128) has an error (code 7): the conversion failed"
        ),
        "output: {}",
        result.combined()
    );
}

#[test]
fn list_filters_by_state() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(1, "1061311664", "queued", 1),
        job_detail(2, "1061311784", "completed", 1),
    ]);

    let mut cmd = env.command();
    cmd.args(["list", "--json", "--states", "queued"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    let parsed = result.stdout_json();
    let jobs = parsed.as_object().expect("a map keyed by job ID");
    assert_eq!(jobs.len(), 1, "only the queued job should be listed");
    assert!(jobs.contains_key("1"));
}

/// `list` sends its date filters, and a single state, to the server.
#[test]
fn list_sends_its_filters_to_the_server() {
    let env = CliEnv::with_session();
    let listing = env.server.mock(|when, then| {
        when.method(httpmock::Method::POST)
            .path("/api/v2/get_jobs")
            .json_body_includes(
                r#"{ "date_from": "2026-09-01T00:00:00Z", "job_state": "completed" }"#,
            );
        then.status(200)
            .json_body(serde_json::json!({ "jobs": [], "total_count": 0 }));
    });

    let mut cmd = env.command();
    cmd.args([
        "list",
        "--json",
        "--date-from",
        "2026-09-01",
        "--job-states",
        "ready",
    ]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(listing.calls(), 1);
}

#[test]
fn list_rejects_job_ids_and_obs_ids_together() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![]);

    let mut cmd = env.command();
    cmd.args(["list", "12345", TEST_OBS_ID]);
    let result = run(cmd);

    assert!(!result.success);
    assert!(
        result.combined().contains("can't specify both"),
        "output: {}",
        result.combined()
    );
}

#[test]
fn a_server_error_on_listing_fails_the_run() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/get_jobs");
        then.status(503).body("service unavailable");
    });

    let mut cmd = env.command();
    cmd.arg("list");
    let result = run(cmd);

    assert!(!result.success, "the run should fail");
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn a_cancellation_is_issued_to_the_server() {
    let env = CliEnv::with_session();
    let cancel = env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/12345");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(12345));
    });

    let mut cmd = env.command();
    cmd.args(["cancel", "12345"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert_eq!(cancel.calls(), 1);
    assert!(
        result
            .combined()
            .contains("Cancel requests: 1 sent, 0 failed."),
        "output: {}",
        result.combined()
    );
}

/// The server answers a cancellation of a job that is already cancelled with
/// a normal (HTTP 200) reply whose `status` is "failed". The CLI can tell it
/// from a success only by the message, so it must not say that the job was
/// cancelled: it logs the server's message as the answer to the request.
#[test]
fn a_cancellation_refused_with_a_normal_reply_is_not_reported_as_cancelled() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/12345");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!({
                "job_id": 12345,
                "message": "Unable to cancel job 12345",
                "status": "failed",
            }));
    });

    let mut cmd = env.command();
    cmd.args(["cancel", "12345"]);
    let result = run(cmd);

    assert!(result.success, "output: {}", result.combined());
    assert!(
        result
            .combined()
            .contains("Cancel request for job 12345: Unable to cancel job 12345"),
        "output: {}",
        result.combined()
    );
    assert!(
        !result.combined().contains("Cancelled"),
        "output: {}",
        result.combined()
    );
    assert!(
        result
            .combined()
            .contains("Cancel requests: 1 sent, 0 failed."),
        "output: {}",
        result.combined()
    );
}

/// A cancellation that the server rejects is logged per job and does not
/// fail the run, so a batch keeps going. Pins that deliberately.
#[test]
fn a_rejected_cancellation_is_logged_without_failing_the_run() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/12345");
        then.status(404)
            .header("content-type", "application/json")
            .json_body(error_response("JOB_NOT_FOUND", "No such job"));
    });

    let mut cmd = env.command();
    cmd.args(["cancel", "12345"]);
    let result = run(cmd);

    assert!(result.success, "the run should not fail");
    assert!(
        result.combined().contains("Failed to cancel"),
        "output: {}",
        result.combined()
    );
    assert!(
        result
            .combined()
            .contains("Cancel requests: 1 sent, 1 failed."),
        "output: {}",
        result.combined()
    );
}

#[test]
fn cancelling_with_no_job_ids_is_rejected() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.arg("cancel");
    let result = run(cmd);

    assert!(!result.success);
    assert_eq!(requests.calls(), 0);
    assert!(
        result.combined().contains("No jobids specified"),
        "output: {}",
        result.combined()
    );
}
