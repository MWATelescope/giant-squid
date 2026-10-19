// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests of where `giant-squid download` puts the files of a job
//! (a directory for each job, named for the Job ID, unless `--no-job-subdir`
//! is given), and of the order in which the downloads start and are
//! numbered in the log (`[1/8]`, `[2/8]`, ...).
//!
//! The built binary downloads small tar files from a local mock server.

mod common;

use common::*;
use httpmock::prelude::*;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tempfile::TempDir;

/// The name of the file in each test tar.
const MEMBER: &str = "1065880128.metafits";

/// A tar file with one member.
fn tar_with(name: &str, contents: &str) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, name, contents.as_bytes())
        .expect("could not build the test tar");
    builder.into_inner().expect("could not finish the test tar")
}

/// The SHA-1 of `bytes`, in hexadecimal.
fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Serve a tar of one member at its own path, and give the completed job
/// that has it as its file.
fn serve_job(env: &CliEnv, job_id: i64, contents: &str) -> Value {
    let tar = tar_with(MEMBER, contents);
    let path = format!("/downloads/{TEST_OBS_ID}_{job_id}_meta.tar");
    env.server.mock(|when, then| {
        when.method(GET).path(path.as_str());
        then.status(200).body(tar.as_slice());
    });
    let mut job = job_detail(job_id, TEST_OBS_ID, "completed", 1);
    job["product"] = json!({
        "files": [{
            "type": "acacia",
            "url": env.server.url(&path),
            "size": tar.len(),
            "sha1": sha1_hex(&tar),
        }]
    });
    job
}

/// The labels `[n/total]` in `log`, each once, in the order that they first
/// appear.
fn labels_in_order(log: &str, total: usize) -> Vec<usize> {
    let suffix = format!("/{total}]");
    let mut labels = Vec::new();
    for line in log.lines() {
        let Some(end) = line.find(&suffix) else {
            continue;
        };
        let Some(start) = line[..end].rfind('[') else {
            continue;
        };
        if let Ok(number) = line[start + 1..end].parse::<usize>() {
            if !labels.contains(&number) {
                labels.push(number);
            }
        }
    }
    labels
}

// ---------------------------------------------------------------------------
// The directory of each job
// ---------------------------------------------------------------------------

#[test]
fn each_job_is_downloaded_into_a_directory_named_for_the_job() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![
        serve_job(&env, 12345, "THE FIRST JOB, WITH LONGER CONTENTS"),
        serve_job(&env, 12346, "THE SECOND JOB"),
    ]);
    let dir = TempDir::new().expect("a download directory");

    let mut cmd = env.command();
    cmd.args(["download", "--json", "-c", "1", "-d"])
        .arg(dir.path())
        .args(["12345", "12346"]);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let read = |job: &str| {
        std::fs::read_to_string(dir.path().join(job).join(MEMBER)).expect("a file for each job")
    };
    assert_eq!(read("12345"), "THE FIRST JOB, WITH LONGER CONTENTS");
    assert_eq!(read("12346"), "THE SECOND JOB");
    assert!(
        !dir.path().join(MEMBER).exists(),
        "nothing in the download directory itself"
    );
    let lines = result.stdout_json_lines();
    assert_eq!(lines.len(), 2, "{}", result.stdout);
    for (line, job) in lines.iter().zip(["12345", "12346"]) {
        let message = line["message"].as_str().unwrap_or_default();
        let expected = dir.path().join(job).display().to_string();
        assert!(
            message.ends_with(&expected),
            "{message} should end with {expected}"
        );
    }
}

#[test]
fn no_job_subdir_downloads_into_the_download_directory_itself() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![serve_job(&env, 12345, "THE JOB")]);
    let dir = TempDir::new().expect("a download directory");

    let mut cmd = env.command();
    cmd.args(["download", "--json", "--no-job-subdir", "-d"])
        .arg(dir.path())
        .arg("12345");
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(MEMBER)).expect("the file in the directory"),
        "THE JOB"
    );
    assert!(
        !dir.path().join("12345").exists(),
        "no directory for the job"
    );
    let message = result.stdout_json()["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        message.ends_with(&dir.path().display().to_string()),
        "{message}"
    );
}

#[test]
fn an_obs_id_is_downloaded_into_the_directory_of_its_job() {
    let env = CliEnv::with_session();
    env.mock_get_jobs(vec![serve_job(&env, 12345, "THE JOB")]);
    let dir = TempDir::new().expect("a download directory");

    let mut cmd = env.command();
    cmd.args(["download", "--json", "-d"])
        .arg(dir.path())
        .arg(TEST_OBS_ID);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert!(dir.path().join("12345").join(MEMBER).is_file());
}

#[test]
fn a_download_directory_that_is_missing_is_still_refused_at_once() {
    let env = CliEnv::with_session();

    let mut cmd = env.command();
    cmd.args(["download", "-d", "/no/such/directory", "12345"]);
    let result = run(cmd);

    assert_eq!(result.code, Some(1), "{}", result.combined());
    assert!(
        result.combined().contains("is not a directory"),
        "{}",
        result.combined()
    );
}

#[test]
fn the_dry_run_says_whether_each_job_gets_a_directory() {
    let env = CliEnv::with_session();
    let dir = TempDir::new().expect("a download directory");

    let mut with_dir = env.command();
    with_dir
        .args(["download", "--dry-run", "--json", "-d"])
        .arg(dir.path())
        .arg("12345");
    let mut without_dir = env.command();
    without_dir
        .args(["download", "--dry-run", "--json", "--no-job-subdir", "-d"])
        .arg(dir.path())
        .arg("12345");
    let (with_dir, without_dir) = (run(with_dir), run(without_dir));

    assert!(with_dir.success && without_dir.success);
    assert_eq!(with_dir.stdout_json()["job_subdir"], true);
    assert_eq!(without_dir.stdout_json()["job_subdir"], false);
}

// ---------------------------------------------------------------------------
// The numbers in the log
// ---------------------------------------------------------------------------

/// A mock with `count` jobs, and the Job IDs of the jobs.
fn serve_many_jobs(env: &CliEnv, count: usize) -> Vec<String> {
    let ids: Vec<i64> = (0..count as i64).map(|i| 100 + i).collect();
    let jobs: Vec<Value> = ids
        .iter()
        .map(|id| serve_job(env, *id, &format!("JOB {id}")))
        .collect();
    env.mock_get_jobs(jobs);
    ids.iter().map(ToString::to_string).collect()
}

/// With one download at a time, the numbers are 1, 2, 3, and so on.
#[test]
fn downloads_one_at_a_time_are_numbered_in_order() {
    let env = CliEnv::with_session();
    let ids = serve_many_jobs(&env, 8);
    let dir = TempDir::new().expect("a download directory");

    let mut cmd = env.command();
    cmd.args(["download", "-c", "1", "-d"])
        .arg(dir.path())
        .args(&ids);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    assert_eq!(
        labels_in_order(&result.stderr, 8),
        (1..=8).collect::<Vec<usize>>(),
        "{}",
        result.stderr
    );
}

/// With several at a time, the first downloads that start are the first of
/// the list. (They used to start at 1, 6, 11 and 16 of 20.)
#[test]
fn the_first_concurrent_downloads_are_the_first_of_the_list() {
    let env = CliEnv::with_session();
    let ids = serve_many_jobs(&env, 12);
    let dir = TempDir::new().expect("a download directory");

    let mut cmd = env.command();
    cmd.args(["download", "-c", "4", "-d"])
        .arg(dir.path())
        .args(&ids);
    let result = run(cmd);

    assert!(result.success, "{}", result.combined());
    let labels = labels_in_order(&result.stderr, 12);
    assert_eq!(labels.len(), 12, "{}", result.stderr);
    let mut first_four = labels[..4].to_vec();
    first_four.sort_unstable();
    assert_eq!(first_four, [1, 2, 3, 4], "{labels:?}");
    // And the rest follow the list: a later number does not appear before a
    // smaller one that starts later than four places after it.
    for (position, label) in labels.iter().enumerate() {
        assert!(
            *label <= position + 4,
            "{label} is too early at {position}: {labels:?}"
        );
    }
}
