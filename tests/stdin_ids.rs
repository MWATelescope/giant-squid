// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests of the IDs that `download`, `cancel` and `wait` read from
//! standard input, and of `list --ids`: the built binary runs against a local
//! mock server, with the text written to its standard input.
//!
//! The reading of standard input has its own unit tests
//! (`src/cli/stdin_ids/tests.rs`), which also cover a terminal. These tests
//! show what only a process shows: a pipe, the output, the exit code, and
//! that a refused input sends no request.

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::*;
use httpmock::{prelude::*, Mock};
use serde_json::{json, Value};

/// A Job ID and an Obs ID that the tests use.
const JOB_ID: u64 = 12345;

/// Run `cmd` with `input` on its standard input, and wait for it to end.
fn run_with_stdin(mut cmd: Command, input: &str) -> Run {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("could not run the giant-squid binary");
    let mut stdin = child.stdin.take().expect("standard input is piped");
    // The command can end before it reads (for a bad argument).
    let _ = stdin.write_all(input.as_bytes());
    drop(stdin);
    let out = child
        .wait_with_output()
        .expect("could not wait for giant-squid");
    Run {
        success: out.status.success(),
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A mock that matches anything, used to prove that no request was made.
fn catch_all(env: &CliEnv) -> Mock<'_> {
    env.server.mock(|when, then| {
        when.any_request();
        then.status(500).body("no request should have reached here");
    })
}

/// The `job_id` of each line of JSON on standard output.
fn job_ids(result: &Run) -> Vec<Value> {
    result
        .stdout_json_lines()
        .iter()
        .map(|line| line["job_id"].clone())
        .collect()
}

// ---------------------------------------------------------------------------
// list --ids
// ---------------------------------------------------------------------------

#[test]
fn list_ids_prints_the_job_id_of_each_job_on_a_line() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(12, TEST_OBS_ID, "completed", 1),
        job_detail(5, TEST_OBS_ID, "queued", 1),
        job_detail(101, TEST_OBS_ID, "error", 1),
    ]);

    let mut cmd = env.command();
    cmd.args(["list", "--ids"]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let mut lines: Vec<&str> = result.stdout.lines().collect();
    lines.sort_unstable();
    assert_eq!(lines, ["101", "12", "5"], "stdout: {}", result.stdout);
}

#[test]
fn list_ids_for_no_job_prints_nothing_to_standard_output() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![]);

    let mut cmd = env.command();
    cmd.args(["list", "--ids"]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(result.stdout, "");
    assert!(
        result.stderr.contains("You have no jobs."),
        "{}",
        result.stderr
    );
}

#[test]
fn list_ids_cannot_be_used_with_json() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    for other in ["--json", "--legacy-json"] {
        let mut cmd = env.command();
        cmd.args(["list", "--ids", other]);
        let result = run(cmd);

        assert_eq!(result.code, Some(2), "{other}: {}", result.combined());
    }
    assert_eq!(requests.calls(), 0);
}

// ---------------------------------------------------------------------------
// download
// ---------------------------------------------------------------------------

/// A download directory that exists, for a dry run.
fn download_dry_run(env: &CliEnv, dir: &tempfile::TempDir, extra: &[&str]) -> Command {
    let mut cmd = env.command();
    cmd.args(["download", "--dry-run", "--json", "-d"])
        .arg(dir.path())
        .args(extra);
    cmd
}

#[test]
fn download_reads_the_ids_of_list_ids_from_standard_input() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");

    let result = run_with_stdin(download_dry_run(&env, &dir, &[]), "12\n5\n");

    assert!(result.success, "{}", result.combined());
    assert_eq!(job_ids(&result), [json!(12), json!(5)]);
}

#[test]
fn download_reads_the_job_ids_of_list_json_in_numeric_order() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");
    let list = json!({
        "100": {"id": 100, "obs_id": 1065880128_u64},
        "9": {"id": 9, "obs_id": 1065880248_u64},
        "10": {"id": 10, "obs_id": 1065880128_u64},
    })
    .to_string();

    let result = run_with_stdin(download_dry_run(&env, &dir, &[]), &list);

    assert!(result.success, "{}", result.combined());
    // The Job IDs are used, not the Obs IDs: an Obs ID can have several jobs.
    assert_eq!(job_ids(&result), [json!(9), json!(10), json!(100)]);
}

#[test]
fn download_reads_job_ids_and_obs_ids_on_standard_input() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");

    let result = run_with_stdin(
        download_dry_run(&env, &dir, &[]),
        &format!("12\n{TEST_OBS_ID}\n"),
    );

    assert!(result.success, "{}", result.combined());
    let lines = result.stdout_json_lines();
    assert_eq!(lines.len(), 2, "{}", result.stdout);
    assert_eq!(lines[0]["job_id"], 12);
    assert_eq!(lines[1]["obs_id"].to_string(), TEST_OBS_ID);
}

#[test]
fn download_does_not_read_standard_input_when_it_has_ids() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");

    let result = run_with_stdin(download_dry_run(&env, &dir, &["77"]), "12\n5\n");

    assert!(result.success, "{}", result.combined());
    assert_eq!(job_ids(&result), [json!(77)]);
}

#[test]
fn a_dash_adds_standard_input_to_the_ids_of_the_arguments() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");

    let result = run_with_stdin(download_dry_run(&env, &dir, &["77", "-"]), "12\n5\n");

    assert!(result.success, "{}", result.combined());
    assert_eq!(job_ids(&result), [json!(77), json!(12), json!(5)]);
}

#[test]
fn download_says_how_many_ids_it_read() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");
    let mut cmd = env.command();
    cmd.args(["download", "--dry-run", "-d"]).arg(dir.path());

    let result = run_with_stdin(cmd, &format!("12\n5\n{TEST_OBS_ID}\n"));

    assert!(result.success, "{}", result.combined());
    assert!(
        result
            .combined()
            .contains("Read 2 Job IDs and 1 Obs IDs from standard input."),
        "{}",
        result.combined()
    );
}

// ---------------------------------------------------------------------------
// cancel and wait
// ---------------------------------------------------------------------------

#[test]
fn cancel_reads_the_ids_of_list_ids_and_cancels_each_job() {
    let env = CliEnv::with_session();
    let first = env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/12");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(12));
    });
    let second = env.server.mock(|when, then| {
        when.method(DELETE).path("/api/v2/jobs/5");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(job_submitted_response(5));
    });

    let mut cmd = env.command();
    cmd.args(["cancel"]);
    let result = run_with_stdin(cmd, "12\n5\n");

    assert!(result.success, "{}", result.combined());
    assert_eq!((first.calls(), second.calls()), (1, 1));
    assert!(
        result
            .stderr
            .contains("Read 2 Job IDs from standard input."),
        "{}",
        result.stderr
    );
}

/// The whole pipe: the output of `list --ids`, run as a process, is the
/// input of `cancel --dry-run`.
#[test]
fn the_output_of_list_ids_is_the_input_of_cancel() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(12, TEST_OBS_ID, "queued", 1),
        job_detail(5, TEST_OBS_ID, "queued", 1),
    ]);
    let mut list = env.command();
    list.args(["list", "--ids"]);
    let listed = run(list);
    assert!(listed.success, "{}", listed.combined());

    let mut cancel = env.command();
    cancel.args(["cancel", "--dry-run", "--json"]);
    let result = run_with_stdin(cancel, &listed.stdout);

    assert!(result.success, "{}", result.combined());
    let mut ids: Vec<u64> = job_ids(&result).iter().filter_map(Value::as_u64).collect();
    ids.sort_unstable();
    assert_eq!(ids, [5, 12]);
}

#[test]
fn cancel_cancels_a_job_that_is_given_more_than_once_a_single_time() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["cancel", "--dry-run", "--json", "5", "5", "-"]);
    let result = run_with_stdin(cmd, "6\n5\n6\n");

    assert!(result.success, "{}", result.combined());
    assert_eq!(job_ids(&result), [json!(5), json!(6)]);
    assert_eq!(requests.calls(), 0);
}

#[test]
fn cancel_refuses_an_obs_id_on_standard_input() {
    let env = CliEnv::with_session();
    let requests = catch_all(&env);

    let mut cmd = env.command();
    cmd.args(["cancel"]);
    let result = run_with_stdin(cmd, &format!("12\n{TEST_OBS_ID}\n"));

    assert!(!result.success);
    assert!(
        result.combined().contains("Expected only Job IDs"),
        "{}",
        result.combined()
    );
    assert_eq!(requests.calls(), 0);
}

#[test]
fn wait_reads_the_job_ids_of_list_json() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![job_detail(JOB_ID as i64, TEST_OBS_ID, "completed", 1)]);
    let list = json!({ JOB_ID.to_string(): {"id": JOB_ID} }).to_string();

    let mut cmd = env.command();
    cmd.args(["wait", "--json"]);
    let result = run_with_stdin(cmd, &list);

    assert!(result.success, "{}", result.combined());
    assert_eq!(result.stdout_json()[JOB_ID.to_string()]["id"], JOB_ID);
}

// ---------------------------------------------------------------------------
// A list with no job, and input that is refused
// ---------------------------------------------------------------------------

/// The commands, with the words that their error has when they get no ID.
const COMMANDS: [(&str, &str); 3] = [
    ("download", "No Job IDs or Obs IDs specified"),
    ("cancel", "No Job IDs specified"),
    ("wait", "No Job IDs specified"),
];

#[test]
fn an_empty_job_list_is_nothing_to_do_and_not_an_error() {
    for (command, _) in COMMANDS {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);

        let mut cmd = env.command();
        cmd.arg(command);
        let result = run_with_stdin(cmd, "{}\n");

        assert!(result.success, "{command}: {}", result.combined());
        assert_eq!(requests.calls(), 0, "{command}");
        assert!(
            result.stderr.contains("Nothing to do."),
            "{command}: {}",
            result.stderr
        );
    }
}

#[test]
fn an_empty_job_list_with_json_prints_nothing() {
    let env = CliEnv::with_session();

    let mut cmd = env.command();
    cmd.args(["cancel", "--json"]);
    let result = run_with_stdin(cmd, "{}");

    assert!(result.success, "{}", result.combined());
    assert_eq!((result.stdout.as_str(), result.stderr.as_str()), ("", ""));
}

#[test]
fn an_empty_pipe_is_nothing_to_do_and_not_an_error() {
    for (command, _) in COMMANDS {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);

        let mut cmd = env.command();
        cmd.arg(command);
        let result = run_with_stdin(cmd, "");

        assert!(result.success, "{command}: {}", result.combined());
        assert_eq!(requests.calls(), 0, "{command}");
        assert!(
            result.stderr.contains("Nothing to do."),
            "{command}: {}",
            result.stderr
        );
    }
}

/// The output of `list --ids` for a list with no job is no text at all. The
/// next command must end with exit code 0, so that a script (cron, for
/// example) does not fail when no job is in the state that it asks for.
#[test]
fn the_empty_output_of_list_ids_is_nothing_to_do_for_every_command() {
    let env = CliEnv::with_session();
    let get_jobs = env.mock_get_jobs(vec![]);
    let mut list = env.command();
    list.args(["list", "--job-states", "completed", "--ids"]);
    let listed = run(list);
    assert!(listed.success, "{}", listed.combined());
    assert_eq!(listed.stdout, "");

    for (command, _) in COMMANDS {
        let mut cmd = env.command();
        cmd.arg(command);
        let result = run_with_stdin(cmd, &listed.stdout);

        assert!(result.success, "{command}: {}", result.combined());
        assert!(
            result.stderr.contains("Nothing to do."),
            "{command}: {}",
            result.stderr
        );
    }
    assert_eq!(get_jobs.calls(), 1, "only `list` asks for the jobs");
}

/// A command that has no ID and no input has been used wrongly. A terminal
/// and `/dev/null` (what cron and a service give) are no input.
#[test]
fn a_command_with_no_id_and_no_input_is_a_mistake() {
    for (command, message) in COMMANDS {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);

        let mut cmd = env.command();
        cmd.arg(command).stdin(Stdio::null());
        let result = run(cmd);

        assert!(!result.success, "{command}");
        assert!(
            result.combined().contains(message),
            "{command}: {}",
            result.combined()
        );
        assert_eq!(requests.calls(), 0, "{command}");
    }
}

#[test]
fn a_file_on_standard_input_is_read_like_a_pipe() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");
    let ids = dir.path().join("ids.txt");
    std::fs::write(&ids, "12\n5\n").expect("could not write the IDs");
    let empty = dir.path().join("empty.txt");
    std::fs::write(&empty, "").expect("could not write the empty file");

    let mut with_ids = download_dry_run(&env, &dir, &[]);
    with_ids.stdin(std::fs::File::open(&ids).expect("the IDs file"));
    let mut with_nothing = download_dry_run(&env, &dir, &[]);
    with_nothing.stdin(std::fs::File::open(&empty).expect("the empty file"));
    let with_ids = run(with_ids);
    let with_nothing = run(with_nothing);

    assert!(with_ids.success, "{}", with_ids.combined());
    assert_eq!(job_ids(&with_ids), [json!(12), json!(5)]);
    assert!(with_nothing.success, "{}", with_nothing.combined());
    assert_eq!(with_nothing.stdout, "");
}

/// Text on standard input is IDs, and nothing else: a word that is not a
/// number is not the name of a file to read.
#[test]
fn a_file_name_on_standard_input_is_refused() {
    let env = CliEnv::with_session();
    let dir = tempfile::TempDir::new().expect("a download directory");
    let ids = dir.path().join("ids.txt");
    std::fs::write(&ids, "12\n").expect("could not write the IDs");

    let result = run_with_stdin(
        download_dry_run(&env, &dir, &[]),
        &format!("{}\n", ids.display()),
    );

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert!(
        result
            .stdout
            .contains("in standard input could not be parsed as an int"),
        "{}",
        result.combined()
    );
}

#[test]
fn the_error_line_of_a_failed_list_is_refused_with_its_message() {
    for (command, _) in COMMANDS {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);
        let error =
            json!({"status": "failed", "error_code": "AUTH", "message": "Bad key"}).to_string();

        let mut cmd = env.command();
        cmd.arg(command);
        let result = run_with_stdin(cmd, &error);

        assert!(!result.success, "{command}");
        assert!(
            result
                .combined()
                .contains("Standard input has an error, not jobs: Bad key"),
            "{command}: {}",
            result.combined()
        );
        assert_eq!(requests.calls(), 0, "{command}");
    }
}

#[test]
fn the_table_of_list_is_refused_and_the_message_names_the_options_to_use() {
    for (command, _) in COMMANDS {
        let env = CliEnv::with_session();
        let requests = catch_all(&env);
        let table = "+--------+------------+\n| Job ID | Obs ID     |\n| 101    | 1065880128 |\n";

        let mut cmd = env.command();
        cmd.arg(command);
        let result = run_with_stdin(cmd, table);

        assert!(!result.success, "{command}");
        assert!(
            result.combined().contains("giant-squid list --ids"),
            "{command}: {}",
            result.combined()
        );
        assert_eq!(requests.calls(), 0, "{command}");
    }
}

#[test]
fn with_json_a_refused_input_is_one_error_line() {
    let env = CliEnv::with_session();

    let mut cmd = env.command();
    cmd.args(["download", "--json"]);
    let result = run_with_stdin(cmd, "not an id\n");

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert_eq!(result.stderr, "");
    let line = result.stdout_json();
    assert_eq!(line["status"], "failed");
    assert!(
        line["message"]
            .as_str()
            .unwrap_or_default()
            .contains("'not' in standard input"),
        "{line}"
    );
}
