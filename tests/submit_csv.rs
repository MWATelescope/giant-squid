// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests of `giant-squid submit-csv`: the built binary reads a
//! manta-ray-client CSV file and submits its jobs to a local mock server.
//!
//! Nothing is submitted to a real MWA ASVO. The parser has its own tests
//! (`src/mwa_asvo/manta_ray_csv/tests.rs`); these show what only a process
//! shows: the files that manta-ray-client users have, the output, the exit
//! code, and that a bad file sends no request.

mod common;

use common::*;
use httpmock::{prelude::*, Mock};
use serde_json::{json, Value};
use tempfile::TempDir;

/// The directory of the manta-ray-client files.
const CSV_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/files/manta-ray-client");

/// The six files, and the endpoint that the jobs of each file go to.
const FILES: [(&str, &str); 6] = [
    ("beamformer_jobs.csv", "/api/v2/beamformer_job"),
    ("conversion_jobs.csv", "/api/v2/conversion_job"),
    ("imaging_jobs.csv", "/api/v2/imaging_job"),
    ("meta_jobs.csv", "/api/v2/download_vis_job"),
    ("visibility_jobs.csv", "/api/v2/download_vis_job"),
    ("voltage_jobs.csv", "/api/v2/voltage_job"),
];

/// A mock that matches anything, used to prove that no request was made.
fn catch_all(env: &CliEnv) -> Mock<'_> {
    env.server.mock(|when, then| {
        when.any_request();
        then.status(500).body("no request should have reached here");
    })
}

/// A CSV file in a temporary directory. The directory lives as long as the
/// value does.
struct CsvFile {
    _dir: TempDir,
    path: String,
}

impl CsvFile {
    fn new(text: &str) -> Self {
        let dir = TempDir::new().expect("could not create a temporary directory");
        let path = dir.path().join("jobs.csv");
        std::fs::write(&path, text).expect("could not write the CSV file");
        Self {
            path: path.display().to_string(),
            _dir: dir,
        }
    }
}

/// Serve `POST endpoint` for the Obs ID `obs_id` with a successful reply.
fn mock_submit<'a>(env: &'a CliEnv, endpoint: &str, obs_id: u64, job_id: u64) -> Mock<'a> {
    env.server.mock(|when, then| {
        when.method(POST)
            .path(endpoint)
            .json_body_includes(json!({ "obs_id": obs_id }).to_string());
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(job_id));
    })
}

// ---------------------------------------------------------------------------
// The files of manta-ray-client users
// ---------------------------------------------------------------------------

#[test]
fn every_manta_ray_file_is_read_and_a_dry_run_sends_nothing() {
    for (file, endpoint) in FILES {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);

        let mut cmd = env.command();
        cmd.args([
            "submit-csv",
            "--dry-run",
            "--json",
            &format!("{CSV_DIR}/{file}"),
        ]);
        let result = run(cmd);

        assert!(result.success, "{file}: {}", result.combined());
        assert_eq!(
            requests.calls(),
            0,
            "{file}: a dry run must not call the API"
        );
        let lines = result.stdout_json_lines();
        assert_eq!(lines.len(), 2, "{file}: {}", result.stdout);
        for line in &lines {
            assert_eq!(line["endpoint"], endpoint, "{file}");
            assert_eq!(line["params"]["obs_id"], line["obs_id"], "{file}");
        }
    }
}

#[test]
fn a_dry_run_without_json_prints_each_body_and_a_summary() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args([
        "submit-csv",
        "-n",
        &format!("{CSV_DIR}/conversion_jobs.csv"),
    ]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(requests.calls(), 0);
    let output = result.combined();
    for expected in [
        "Would POST /api/v2/conversion_job for Obs ID 1349904240 (line 1)",
        "\"output\": \"uvfits\"",
        "\"custom_centre_ra\": 123.0",
        "Would have submitted 2 jobs. Nothing was sent.",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
}

/// The jobs of one file can be of every type. Each is submitted to its own
/// endpoint with the body that its line makes, in the order of the file.
#[test]
fn the_jobs_of_a_file_are_submitted_to_their_own_endpoints() {
    let env = CliEnv::with_session();
    let conversion = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/conversion_job")
            .json_body_includes(
                json!({"obs_id": 1349904240_u64, "output": "uvfits", "no_rfi": true, "avg_freq_res": 80.0})
                    .to_string(),
            );
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(101));
    });
    let visibility = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(
                json!({"obs_id": 1349904184_u64, "download_type": "vis"}).to_string(),
            );
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(102));
    });
    let metadata = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(
                json!({"obs_id": 1349904185_u64, "download_type": "meta"}).to_string(),
            );
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(103));
    });
    let voltage = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/voltage_job")
            .json_body_includes(
                json!({"obs_id": 1323507256_u64, "from_channel": 185, "channel_range": true})
                    .to_string(),
            );
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(104));
    });
    let imaging = env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/imaging_job")
            .json_body_includes(json!({"obs_id": 1349904240_u64, "image_size": 3072}).to_string());
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(105));
    });
    let beamformer = mock_submit(&env, "/api/v2/beamformer_job", 1457858440, 106);
    let file = CsvFile::new(
        "# one of each\n\
         obs_id=1349904240, job_type=c, avg_freq_res=80.0, output=uvfits, no_rfi=true\n\
         obs_id=1349904184, job_type=d, download_type=vis\n\
         obs_id=1349904185, job_type=d, download_type=vis_meta\n\
         obs_id=1323507256, job_type=v, offset=0, duration=16, from_channel=185, to_channel=185\n\
         obs_id=1349904240, job_type=i, image_size=3072\n\
         obs_id=1457858440, job_type=b\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--json", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(result.stderr, "");
    let job_ids: Vec<Value> = result
        .stdout_json_lines()
        .iter()
        .map(|l| l["job_id"].clone())
        .collect();
    assert_eq!(job_ids, [101, 102, 103, 104, 105, 106].map(Value::from));
    for mock in [
        conversion, visibility, metadata, voltage, imaging, beamformer,
    ] {
        assert_eq!(mock.calls(), 1);
    }
}

// ---------------------------------------------------------------------------
// A bad file submits nothing
// ---------------------------------------------------------------------------

#[test]
fn a_bad_line_submits_nothing_and_every_bad_line_is_reported() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis\n\
         obs_id=1349904184, job_type=c, no_rf=true\n\
         \n\
         obs_id=1349904185, job_type=c, flag_edge_width=9999\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert_eq!(requests.calls(), 0, "a bad file must send no request");
    let output = result.combined();
    for expected in [
        "2 problems in the CSV file",
        "line 2: unknown key `no_rf`",
        "line 4: ",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
}

#[test]
fn a_bad_file_with_json_prints_one_error_line() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);
    let file = CsvFile::new("obs_id=1349904240, job_type=x\n");

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--json", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert_eq!(requests.calls(), 0);
    let line = result.stdout_json();
    assert_eq!(line["status"], "failed");
    assert_eq!(line["line"], 1);
    assert!(
        line["message"]
            .as_str()
            .unwrap_or_default()
            .contains("job_type"),
        "{line}"
    );
}

#[test]
fn a_bad_file_with_json_prints_an_error_line_for_each_bad_line() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);
    let file = CsvFile::new(
        "# a comment\n\
         obs_id=1349904240, job_type=x\n\
         obs_id=1349904184, job_type=d, download_type=vis\n\
         obs_id=abc, job_type=c\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--json", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert_eq!(result.stderr, "");
    assert_eq!(requests.calls(), 0);
    let lines = result.stdout_json_lines();
    assert_eq!(lines.len(), 2, "{}", result.stdout);
    for line in &lines {
        assert_eq!(line["status"], "failed");
        assert_eq!(line["error_code"], "INVALID_ARGUMENT");
    }
    assert_eq!(lines[0]["line"], 2);
    assert_eq!(lines[1]["line"], 4);
    assert!(
        lines[1]["message"]
            .as_str()
            .unwrap_or_default()
            .starts_with("obs_id: "),
        "{}",
        lines[1]
    );
}

#[test]
fn a_bad_line_is_refused_also_with_a_dry_run() {
    let env = CliEnv::with_session();
    let file = CsvFile::new("obs_id=1349904240, job_type=d\n");

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--dry-run", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert!(
        result.combined().contains("download_type is required"),
        "{}",
        result.combined()
    );
}

#[test]
fn a_file_that_is_missing_or_has_no_job_is_an_error() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);
    let empty = CsvFile::new("# nothing here\n\n");

    let mut cmd = env.command();
    cmd.args(["submit-csv", &format!("{CSV_DIR}/no_such_file.csv")]);
    let missing = run(cmd);
    let mut cmd = env.command();
    cmd.args(["submit-csv", &empty.path]);
    let no_jobs = run(cmd);

    assert_eq!(missing.code, Some(1));
    assert!(
        missing.combined().contains("Could not read the CSV file"),
        "{}",
        missing.combined()
    );
    assert_eq!(no_jobs.code, Some(1));
    assert!(
        no_jobs.combined().contains("has no jobs"),
        "{}",
        no_jobs.combined()
    );
    assert_eq!(requests.calls(), 0);
}

// ---------------------------------------------------------------------------
// A failed submission does not stop the others
// ---------------------------------------------------------------------------

#[test]
fn a_failed_submission_does_not_stop_the_other_jobs() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(json!({"obs_id": 1349904240_u64}).to_string());
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("VALID_MISSING_FIELD", "Bad obsid"));
    });
    let second = mock_submit(&env, "/api/v2/download_vis_job", 1349904184, 202);
    let third = mock_submit(&env, "/api/v2/conversion_job", 1349904185, 203);
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis\n\
         obs_id=1349904184, job_type=d, download_type=vis\n\
         obs_id=1349904185, job_type=c\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert_eq!(second.calls(), 1);
    assert_eq!(third.calls(), 1);
    let output = result.combined();
    for expected in [
        "Line 1: Obs ID 1349904240",
        "Line 2: submitted 1349904184 as MWA ASVO Job ID 202",
        "Submitted 2 of 3 jobs.",
        "1 of 3 job submissions failed",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
}

#[test]
fn with_json_a_failed_submission_is_an_error_line_with_its_obs_id() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/download_vis_job");
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("VALID_MISSING_FIELD", "Bad obsid"));
    });
    let file = CsvFile::new("obs_id=1349904240, job_type=d, download_type=vis\n");

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--json", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1));
    assert_eq!(result.stderr, "");
    let lines = result.stdout_json_lines();
    assert_eq!(lines.len(), 1, "{}", result.stdout);
    assert_eq!(lines[0]["status"], "failed");
    assert_eq!(lines[0]["error_code"], "VALID_MISSING_FIELD");
    assert_eq!(lines[0]["obs_id"], 1349904240_u64);
}

#[test]
fn with_json_only_the_error_line_of_a_job_has_the_line_of_the_file() {
    let env = CliEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(json!({"obs_id": 1349904240_u64}).to_string());
        then.status(400)
            .header("content-type", "application/json")
            .json_body(error_response("VALID_MISSING_FIELD", "Bad obsid"));
    });
    env.server.mock(|when, then| {
        when.method(POST)
            .path("/api/v2/download_vis_job")
            .json_body_includes(json!({"obs_id": 1349904184_u64}).to_string());
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(21));
    });
    let file = CsvFile::new(
        "# a comment\n\
         obs_id=1349904184, job_type=d, download_type=vis\n\
         obs_id=1349904240, job_type=d, download_type=vis\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--json", &file.path]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1));
    let lines = result.stdout_json_lines();
    assert_eq!(lines.len(), 2, "{}", result.stdout);
    // A submitted job: the reply, as the other submit commands print it.
    assert_eq!(lines[0]["status"], "success");
    assert_eq!(lines[0]["job_id"], 21);
    assert!(lines[0].get("line").is_none(), "{}", lines[0]);
    assert!(lines[0].get("obs_id").is_none(), "{}", lines[0]);
    // A job that failed: the error line, with its Obs ID and its line.
    assert_eq!(lines[1]["status"], "failed");
    assert_eq!(lines[1]["obs_id"], 1349904240_u64);
    assert_eq!(lines[1]["line"], 3);
}

// ---------------------------------------------------------------------------
// Defaults: the environment and --allow-resubmit
// ---------------------------------------------------------------------------

#[test]
fn the_delivery_variables_fill_the_lines_that_leave_them_out() {
    let env = CliEnv::with_session();
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis\n\
         obs_id=1349904184, job_type=d, download_type=vis, delivery=dug, delivery_format=tar\n",
    );

    let mut cmd = env.command();
    cmd.env("GIANT_SQUID_DELIVERY", "scratch")
        .env("GIANT_SQUID_DELIVERY_FORMAT", "files")
        .args(["submit-csv", "-n", "--json", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let lines = result.stdout_json_lines();
    assert_eq!(lines[0]["params"]["delivery"], "scratch");
    assert_eq!(lines[0]["params"]["delivery_format"], "files");
    assert_eq!(lines[1]["params"]["delivery"], "dug");
    assert_eq!(lines[1]["params"]["delivery_format"], "tar");
}

#[test]
fn a_bad_delivery_variable_is_an_error_that_names_it() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.env("GIANT_SQUID_DELIVERY", "nowhere").args([
        "submit-csv",
        "-n",
        &format!("{CSV_DIR}/meta_jobs.csv"),
    ]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert!(
        result.combined().contains("GIANT_SQUID_DELIVERY='nowhere'"),
        "{}",
        result.combined()
    );
    assert_eq!(requests.calls(), 0);
}

#[test]
fn allow_resubmit_applies_to_the_lines_that_do_not_give_it() {
    let env = CliEnv::with_session();
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis\n\
         obs_id=1349904184, job_type=d, download_type=vis, allow_resubmit=false\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "-n", "--json", "--allow-resubmit", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let lines = result.stdout_json_lines();
    assert_eq!(lines[0]["params"]["allow_resubmit"], true);
    assert_eq!(lines[1]["params"]["allow_resubmit"], false);
}

// ---------------------------------------------------------------------------
// --wait
// ---------------------------------------------------------------------------

/// With `--wait`, the command waits for the jobs that it submitted, as the
/// other submit commands do.
#[test]
fn wait_waits_for_the_jobs_that_were_submitted() {
    let env = CliEnv::with_session();
    let submit = mock_submit(&env, "/api/v2/download_vis_job", 1065880128, 12345);
    let jobs = env.mock_get_jobs(vec![job_detail(12345, TEST_OBS_ID, "completed", 1)]);
    let file = CsvFile::new("obs_id=1065880128, job_type=d, download_type=vis\n");

    let mut cmd = env.command();
    cmd.args(["submit-csv", "--wait", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(submit.calls(), 1);
    assert!(jobs.calls() >= 1, "the command should have listed the jobs");
}

// ---------------------------------------------------------------------------
// Warnings, and the other keys of the schema
// ---------------------------------------------------------------------------

/// A key given twice and a repeated job are warnings, not errors: every job
/// is still read. With `--json` no log is printed, so the warnings are not.
#[test]
fn with_json_the_warnings_are_not_printed_and_every_job_is_read() {
    let env = CliEnv::with_session();
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis, download_type=vis_meta\n\
         obs_id=1349904240, job_type=d, download_type=vis_meta\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "-n", "--json", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(result.stderr, "");
    assert_eq!(result.stdout_json_lines().len(), 2);
}

#[test]
fn a_warning_is_shown_without_json() {
    let env = CliEnv::with_session();
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=d, download_type=vis, download_type=vis_meta\n\
         obs_id=1349904240, job_type=d, download_type=vis_meta\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "-n", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let output = result.combined();
    for expected in [
        "[WARN]",
        "line 1: the key `download_type` is given twice; the last value is used",
        "line 2: this line makes the same job as line 1",
        "Would have submitted 2 jobs.",
    ] {
        assert!(output.contains(expected), "missing {expected}: {output}");
    }
}

#[test]
fn the_other_fields_of_the_schema_are_accepted_as_keys() {
    let env = CliEnv::with_session();
    let file = CsvFile::new(
        "obs_id=1349904240, job_type=i, pol=xx_yy, join_channels=true, abs_threshold=0.5, wstack_nwlayers=64, no_rfi=true\n\
         obs_id=1349904240, job_type=c, no_apply_amps=true\n\
         obs_id=1457858440, job_type=b, mode=MWAX_CORR_BF\n",
    );

    let mut cmd = env.command();
    cmd.args(["submit-csv", "-n", "--json", &file.path]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let lines = result.stdout_json_lines();
    assert_eq!(lines[0]["params"]["pol"], "XXYY");
    assert_eq!(lines[0]["params"]["wstack_nwlayers"], 64);
    assert_eq!(lines[1]["params"]["no_apply_amps"], true);
    assert_eq!(lines[2]["params"]["mode"], "MWAX_CORR_BF");
}
