// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Live end-to-end tests: the built `giant-squid` binary is run against a
//! real MWA ASVO server.
//!
//! Every test here is `#[ignore]`d, so CI never runs them. Run them by hand:
//!
//! ```text
//! MWA_ASVO_E2E_TARGET=https://test-asvo.mwatelescope.org \
//! MWA_ASVO_API_KEY=<your key> \
//!   cargo test --test live -- --ignored --nocapture
//! ```
//!
//! Safety guards:
//!
//! - The server comes only from `MWA_ASVO_E2E_TARGET`. The production host
//!   is refused unless `MWA_ASVO_E2E_ALLOW_PRODUCTION=1` is also set.
//! - The real token cache (`~/.mwa-asvo`, shared with mwa-cli) is never
//!   read or written. The tests use their own `HOME` under Cargo's
//!   `CARGO_TARGET_TMPDIR`, one per target host and API key.
//! - The server rate-limits logins (5 per minute), so the tests share that
//!   `HOME` and log in at most once per run; a later run reuses the cached
//!   session while it is valid. Only the two authentication tests use a
//!   fresh `HOME`, and so log in themselves. The tests hold a lock while
//!   they run, because a login or token refresh rewrites the shared cache.
//! - Every submission passes `--allow-resubmit`, except the one test that
//!   checks the duplicate check itself.
//! - Every job a test submits is cancelled when the test ends, pass or
//!   fail, by [`JobGuard`].
//! - Nothing is downloaded: a submitted job is not ready in the time a test
//!   runs.
//!
//! Rejections: where the schema documents the error code (`JOB_NOT_FOUND`,
//! `AUTH_REQUIRED`), or the client depends on it (`AUTH_INVALID_TOKEN`), the
//! test asserts the exact code. Otherwise it asserts that the server sent a
//! structured `ErrorResponse`, and prints the code so it can be pinned later.
//!
//! See docs/TESTING.md, layer 3.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde_json::Value;
use sha1::{Digest, Sha1};
use tempfile::{NamedTempFile, TempDir};

use common::{jwt_expiring_in, run, write_session_at, Run};

/// The server to test against. Required.
const ENV_E2E_TARGET: &str = "MWA_ASVO_E2E_TARGET";
/// Set to `1` to allow [`PRODUCTION_HOST`] as the target.
const ENV_E2E_ALLOW_PRODUCTION: &str = "MWA_ASVO_E2E_ALLOW_PRODUCTION";
/// The value of [`ENV_E2E_ALLOW_PRODUCTION`] that allows production.
const ALLOW_PRODUCTION_VALUE: &str = "1";
/// The caller's real API key.
const ENV_API_KEY: &str = "MWA_ASVO_API_KEY";
/// The production MWA ASVO host name.
const PRODUCTION_HOST: &str = "asvo.mwatelescope.org";

/// A small obsid, cheap for the server to process.
const LIVE_TEST_OBSID: &str = "1384357952";
/// An obsid with voltage data.
const LIVE_VOLTAGE_OBSID: &str = "1360706032";
/// An obsid with beamformer data.
const LIVE_BEAMFORMER_OBSID: &str = "1465549448";
/// A well-formed obsid with no data behind it.
const LIVE_NO_DATA_OBSID: &str = "1000000000";
/// A job ID that no user has.
const LIVE_UNKNOWN_JOB_ID: &str = "999999999";
/// Voltage job range: a short slice from the start of the observation.
const LIVE_VOLTAGE_OFFSET_SECS: &str = "0";
const LIVE_VOLTAGE_DURATION_SECS: &str = "8";
/// A look-back window for `list --days`.
const LIVE_LIST_DAYS: &str = "7";
/// A short look-back window, for the one request that primes the session.
/// The smallest value the server accepts: the schema limits `days` to
/// more than 1 and at most 30.
const LIVE_LOGIN_LIST_DAYS: &str = "2";

/// The directory, under `CARGO_TARGET_TMPDIR`, holding the shared `HOME`s.
const LIVE_HOME_DIR: &str = "live-home";
/// How many hex digits of the API key's SHA1 name its `HOME`. Enough to
/// keep accounts apart without writing the key itself to disk.
const API_KEY_ID_LEN: usize = 12;

/// Serialises the tests. They share one token cache, and a login or token
/// refresh rewrites it.
static LIVE_LOCK: Mutex<()> = Mutex::new(());
/// The outcome of the one shared login, so that a failed login is reported
/// by every test without another request to the rate-limited endpoint.
static SHARED_SESSION: OnceLock<Result<(), String>> = OnceLock::new();

/// How the CLI renders a structured `ErrorResponse` (see `AsvoApiError`).
const API_ERROR_PREFIX: &str = "MWA ASVO returned an error (";
/// How the CLI renders a failed login.
const AUTH_FAILED_PREFIX: &str = "Authentication with MWA ASVO failed";
/// Error codes named in the OpenAPI schema or relied on by the client.
const ERR_JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";
const AUTH_ERROR_CODES: [&str; 2] = ["AUTH_INVALID_TOKEN", "AUTH_REQUIRED"];
/// An API key the server cannot know.
const INVALID_API_KEY: &str = "not-a-real-api-key";

/// Job type and state names as `list --json` prints them.
const TYPE_CONVERSION: &str = "Conversion";
const TYPE_DOWNLOAD_VIS: &str = "DownloadVisibilities";
const TYPE_DOWNLOAD_META: &str = "DownloadMetadata";
const TYPE_DOWNLOAD_VOLTAGE: &str = "DownloadVoltage";
const TYPE_DOWNLOAD_BEAMFORMER: &str = "DownloadBeamformer";
const TYPE_IMAGING: &str = "Imaging";
const STATE_CANCELLED: &str = "Cancelled";
const STATE_ERROR: &str = "Error";
/// States in which a job does no more work, so needs no cancelling.
const FINISHED_STATES: [&str; 4] = [STATE_CANCELLED, STATE_ERROR, "Ready", "Expired"];

/// A real MWA ASVO, with a test-only `HOME` for the token cache.
struct LiveEnv {
    target: String,
    api_key: String,
    home: PathBuf,
    /// Set when `home` is a fresh temporary directory, to remove it on drop.
    _own_home: Option<TempDir>,
    /// Held for the life of the test; see [`LIVE_LOCK`].
    _lock: MutexGuard<'static, ()>,
}

impl LiveEnv {
    /// Use the shared `HOME`, logging in first if this run has not yet.
    fn new() -> Self {
        let lock = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (target, api_key) = target_and_api_key();
        let home = shared_home(&target, &api_key);
        let env = Self {
            target,
            api_key,
            home,
            _own_home: None,
            _lock: lock,
        };
        if let Err(output) = SHARED_SESSION.get_or_init(|| env.prime_session()) {
            panic!("the shared login failed, so this test cannot run: {output}");
        }
        env
    }

    /// Use a fresh, empty `HOME`, for a test that must control the token
    /// cache itself. The first command in it logs in.
    fn isolated() -> Self {
        let lock = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (target, api_key) = target_and_api_key();
        let own_home = TempDir::new().expect("could not create a temporary HOME");
        Self {
            target,
            api_key,
            home: own_home.path().to_path_buf(),
            _own_home: Some(own_home),
            _lock: lock,
        }
    }

    /// Make one authenticated request, which logs in only if the cache has
    /// no usable session.
    fn prime_session(&self) -> Result<(), String> {
        let result = self.run(&["list", "--days", LIVE_LOGIN_LIST_DAYS]);
        if result.success {
            Ok(())
        } else {
            Err(result.combined())
        }
    }

    /// The built binary, pointed at the target. Every variable the client
    /// reads is set or cleared, so the caller's own settings cannot leak in.
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_giant-squid"));
        cmd.env("MWA_ASVO_HOST", &self.target)
            .env(ENV_API_KEY, &self.api_key)
            .env("HOME", &self.home)
            .env_remove("MWA_ASVO_API_TIMEOUT")
            .env_remove("GIANT_SQUID_DELIVERY")
            .env_remove("GIANT_SQUID_DELIVERY_FORMAT")
            .env_remove("GIANT_SQUID_BUF_SIZE");
        cmd
    }

    /// Run the binary with `args`.
    fn run(&self, args: &[&str]) -> Run {
        let mut cmd = self.command();
        cmd.args(args);
        run(cmd)
    }

    /// Submit with `--json` and return the new job IDs. Each ID is given
    /// to `guard` before anything is asserted, so it is cancelled even if
    /// the submission partly failed.
    fn submit(&self, guard: &mut JobGuard, args: &[&str]) -> Vec<u64> {
        let mut cmd = self.command();
        cmd.args(args).arg("--json");
        self.submit_command(guard, cmd)
    }

    /// As [`Self::submit`], for a caller-built command (which must already
    /// carry `--json`).
    fn submit_command(&self, guard: &mut JobGuard, cmd: Command) -> Vec<u64> {
        let result = run(cmd);
        let ids = submitted_job_ids(&result);
        ids.iter().for_each(|id| guard.track(*id));
        assert!(result.success, "submission failed: {}", result.combined());
        assert!(!ids.is_empty(), "no job IDs on stdout: {}", result.stdout);
        ids
    }

    /// Submit something the server should reject, and return the error
    /// code it sent. Any job that is created anyway is still cleaned up.
    fn submit_expecting_rejection(&self, guard: &mut JobGuard, args: &[&str]) -> String {
        let mut cmd = self.command();
        cmd.args(args).arg("--json");
        let result = run(cmd);
        let ids = submitted_job_ids(&result);
        ids.iter().for_each(|id| guard.track(*id));
        assert!(
            ids.is_empty() && !result.success,
            "expected the server to reject {args:?}: {}",
            result.combined()
        );
        let code = api_error_code(&result.combined())
            .unwrap_or_else(|| panic!("expected a structured error: {}", result.combined()));
        eprintln!("{args:?} rejected with {code}");
        code
    }

    /// One job as `list --json <id>` reports it.
    fn listed_job(&self, id: u64) -> Value {
        let id = id.to_string();
        let result = self.run(&["list", "--json", &id]);
        assert!(result.success, "list failed: {}", result.combined());
        let job = result.stdout_json()[&id].clone();
        assert!(
            !job.is_null(),
            "job {id} is not in the listing: {}",
            result.stdout
        );
        job
    }

    /// Check that `id` is listed as a live job of `job_type` for `obsid`.
    fn assert_submitted(&self, id: u64, obsid: &str, job_type: &str) {
        let job = self.listed_job(id);
        assert_eq!(
            job["obsid"].as_u64().map(|o| o.to_string()).as_deref(),
            Some(obsid),
            "job: {job}"
        );
        assert_eq!(job["jobType"], job_type, "job: {job}");
        let state = state_name(&job);
        assert!(
            state != STATE_ERROR && state != STATE_CANCELLED,
            "job {id} is already {state}: {job}"
        );
    }
}

/// The jobs a test submitted. Dropping the guard cancels whatever is left,
/// so a failed assertion does not leave work queued on the server.
struct JobGuard<'a> {
    env: &'a LiveEnv,
    ids: Vec<u64>,
}

impl<'a> JobGuard<'a> {
    fn new(env: &'a LiveEnv) -> Self {
        Self {
            env,
            ids: Vec::new(),
        }
    }

    fn track(&mut self, id: u64) {
        self.ids.push(id);
    }

    /// Stop tracking `id`, for a test that cancels it itself.
    fn forget(&mut self, id: u64) {
        self.ids.retain(|i| *i != id);
    }
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        let ids: Vec<String> = self.ids.iter().map(u64::to_string).collect();
        let mut cmd = self.env.command();
        cmd.arg("cancel").args(&ids);
        let cancel = run(cmd);

        // `cancel` logs a failure for one job and carries on with a zero
        // exit code, so check what the server now says about each job.
        let mut list_args = vec!["list", "--json"];
        list_args.extend(ids.iter().map(String::as_str));
        let listing = self.env.run(&list_args);
        let jobs = if listing.success {
            listing.stdout_json()
        } else {
            Value::Null
        };
        let leaked: Vec<&String> = ids
            .iter()
            .filter(|id| !FINISHED_STATES.contains(&state_name(&jobs[id.as_str()]).as_str()))
            .collect();
        if leaked.is_empty() {
            return;
        }
        let message = format!(
            "jobs {leaked:?} may still be active on the server; cancel them by hand.\ncancel: {}\nlist: {}",
            cancel.combined(),
            listing.combined()
        );
        // Panicking while already unwinding would abort the whole run.
        if std::thread::panicking() {
            eprintln!("{message}");
        } else {
            panic!("{message}");
        }
    }
}

/// The target and API key from the environment, refusing production unless
/// it is explicitly allowed.
fn target_and_api_key() -> (String, String) {
    let target = std::env::var(ENV_E2E_TARGET)
        .unwrap_or_else(|_| panic!("set {ENV_E2E_TARGET} to the MWA ASVO to test against"));
    let allow_production =
        std::env::var(ENV_E2E_ALLOW_PRODUCTION).is_ok_and(|v| v == ALLOW_PRODUCTION_VALUE);
    assert!(
        host_of(&target) != PRODUCTION_HOST || allow_production,
        "{target} is production; set {ENV_E2E_ALLOW_PRODUCTION}={ALLOW_PRODUCTION_VALUE} to allow it"
    );
    let api_key = std::env::var(ENV_API_KEY)
        .unwrap_or_else(|_| panic!("set {ENV_API_KEY} to your MWA ASVO API key"));
    (target, api_key)
}

/// The shared `HOME` for a target and API key. Keyed by both, so a cached
/// session is never used against another server or for another account.
fn shared_home(target: &str, api_key: &str) -> PathBuf {
    let key_id = format!("{:x}", Sha1::digest(api_key.as_bytes()));
    let home = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(LIVE_HOME_DIR)
        .join(format!("{}-{}", host_of(target), &key_id[..API_KEY_ID_LEN]));
    std::fs::create_dir_all(&home).expect("could not create the shared test HOME");
    home
}

/// The host part of a URL such as `https://host:443/path`.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', ':']).next().unwrap_or(rest)
}

/// The job IDs from the `--json` lines a submission printed.
fn submitted_job_ids(result: &Run) -> Vec<u64> {
    result
        .stdout
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .filter_map(|v| v["job_id"].as_u64())
        .collect()
}

/// The code in the first structured API error in `output`.
fn api_error_code(output: &str) -> Option<String> {
    let start = output.find(API_ERROR_PREFIX)? + API_ERROR_PREFIX.len();
    let end = output[start..].find(')')? + start;
    Some(output[start..end].to_string())
}

/// A listed job's state name. `Error` carries its message, so it
/// serialises as `{"Error": "..."}` rather than a plain string.
fn state_name(job: &Value) -> String {
    match &job["jobState"] {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.keys().next().cloned().unwrap_or_default(),
        other => other.to_string(),
    }
}

/// A temporary file holding `obsid`, for the obsids-from-a-file form.
fn obsid_file(obsid: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("could not create an obsid file");
    writeln!(file, "{obsid}").expect("could not write the obsid file");
    file
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_list_forms() {
    let env = LiveEnv::new();

    for args in [
        vec!["list"],
        vec!["l"],
        vec!["list", "--days", LIVE_LIST_DAYS],
    ] {
        let result = env.run(&args);
        assert!(result.success, "{args:?}: {}", result.combined());
    }

    let result = env.run(&["list", "--json"]);
    assert!(result.success, "{}", result.combined());
    assert!(result.stdout_json().is_object());

    let result = env.run(&["list", "--json", "--states", "queued,ready"]);
    assert!(result.success, "{}", result.combined());
    for job in result.stdout_json().as_object().unwrap().values() {
        let state = state_name(job);
        assert!(
            state == "Queued" || state == "Ready",
            "unexpected state: {job}"
        );
    }

    let result = env.run(&["list", "--json", "--types", "download_metadata"]);
    assert!(result.success, "{}", result.combined());
    for job in result.stdout_json().as_object().unwrap().values() {
        assert_eq!(job["jobType"], TYPE_DOWNLOAD_META, "unexpected type: {job}");
    }
}

// ---------------------------------------------------------------------------
// Submission, one test per command
// ---------------------------------------------------------------------------

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_vis() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);
    let file = obsid_file(LIVE_TEST_OBSID);
    let file = file.path().to_str().unwrap();

    for args in [
        vec!["submit-vis", "-r", LIVE_TEST_OBSID],
        vec!["sv", "-r", file],
    ] {
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_TEST_OBSID, TYPE_DOWNLOAD_VIS);
        }
    }
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_meta() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    for args in [
        vec!["submit-meta", "-r", LIVE_TEST_OBSID],
        vec!["sm", "-r", LIVE_TEST_OBSID],
    ] {
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_TEST_OBSID, TYPE_DOWNLOAD_META);
        }
    }
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_conv() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    for args in [
        vec!["submit-conv", "-r", LIVE_TEST_OBSID],
        vec!["sc", "-r", "--output", "ms", LIVE_TEST_OBSID],
    ] {
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_TEST_OBSID, TYPE_CONVERSION);
        }
    }
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_image() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    for args in [
        vec!["submit-image", "-r", LIVE_TEST_OBSID],
        vec!["si", "-r", LIVE_TEST_OBSID],
    ] {
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_TEST_OBSID, TYPE_IMAGING);
        }
    }
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_volt() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);
    let range = [
        "--offset",
        LIVE_VOLTAGE_OFFSET_SECS,
        "--duration",
        LIVE_VOLTAGE_DURATION_SECS,
    ];

    for command in ["submit-volt", "st"] {
        let mut args = vec![command, "-r"];
        args.extend(range);
        args.push(LIVE_VOLTAGE_OBSID);
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_VOLTAGE_OBSID, TYPE_DOWNLOAD_VOLTAGE);
        }
    }
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submit_bf() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    for args in [
        vec!["submit-bf", "-r", LIVE_BEAMFORMER_OBSID],
        vec!["sb", "-r", LIVE_BEAMFORMER_OBSID],
    ] {
        for id in env.submit(&mut guard, &args) {
            env.assert_submitted(id, LIVE_BEAMFORMER_OBSID, TYPE_DOWNLOAD_BEAMFORMER);
        }
    }
}

/// The delivery defaults can come from the environment, and the server
/// accepts what they produce.
#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_delivery_env_defaults() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    let mut cmd = env.command();
    cmd.env("GIANT_SQUID_DELIVERY", "acacia")
        .env("GIANT_SQUID_DELIVERY_FORMAT", "tar")
        .args(["submit-meta", "-r", "--json", LIVE_TEST_OBSID]);
    for id in env.submit_command(&mut guard, cmd) {
        env.assert_submitted(id, LIVE_TEST_OBSID, TYPE_DOWNLOAD_META);
    }
}

// ---------------------------------------------------------------------------
// Rejected submissions
// ---------------------------------------------------------------------------

/// Without `--allow-resubmit`, a job already in the user's list is refused.
#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_duplicate_submission_is_rejected_without_allow_resubmit() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    env.submit(&mut guard, &["submit-meta", "-r", LIVE_TEST_OBSID]);
    env.submit_expecting_rejection(&mut guard, &["submit-meta", LIVE_TEST_OBSID]);
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_submission_for_an_obsid_with_no_data_is_rejected() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    env.submit_expecting_rejection(&mut guard, &["submit-vis", "-r", LIVE_NO_DATA_OBSID]);
}

/// Imaging from a job needs a completed conversion job; one just
/// submitted is not complete yet.
#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_image_from_an_unfinished_conversion_is_rejected() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    let conv_id = env.submit(&mut guard, &["submit-conv", "-r", LIVE_TEST_OBSID])[0].to_string();
    env.submit_expecting_rejection(
        &mut guard,
        &[
            "submit-image-from-job",
            "-r",
            "--source-job-id",
            &conv_id,
            LIVE_TEST_OBSID,
        ],
    );
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_image_from_an_unknown_job_is_rejected() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);

    env.submit_expecting_rejection(
        &mut guard,
        &[
            "sifj",
            "-r",
            "--source-job-id",
            LIVE_UNKNOWN_JOB_ID,
            LIVE_TEST_OBSID,
        ],
    );
}

// ---------------------------------------------------------------------------
// Cancellation and waiting
// ---------------------------------------------------------------------------

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_cancel() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);
    let id = env.submit(&mut guard, &["submit-meta", "-r", LIVE_TEST_OBSID])[0];
    let id_str = id.to_string();

    let result = env.run(&["cancel", &id_str]);
    assert!(result.success, "{}", result.combined());
    assert!(
        result.combined().contains("Cancelled 1 jobs."),
        "{}",
        result.combined()
    );
    guard.forget(id);
    assert_eq!(state_name(&env.listed_job(id)), STATE_CANCELLED);

    // A second cancellation is refused by the server; the CLI logs it and
    // carries on rather than failing the run.
    let result = env.run(&["cancel", &id_str]);
    assert!(result.success, "{}", result.combined());
    let code = api_error_code(&result.combined())
        .unwrap_or_else(|| panic!("expected a structured error: {}", result.combined()));
    eprintln!("second cancellation rejected with {code}");
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_cancelling_an_unknown_job_reports_job_not_found() {
    let env = LiveEnv::new();

    let result = env.run(&["cancel", LIVE_UNKNOWN_JOB_ID]);
    assert!(result.success, "{}", result.combined());
    assert_eq!(
        api_error_code(&result.combined()).as_deref(),
        Some(ERR_JOB_NOT_FOUND),
        "{}",
        result.combined()
    );
}

/// `wait` gives up at once on a cancelled job, so this needs no long wait.
#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_wait_on_a_cancelled_job_fails() {
    let env = LiveEnv::new();
    let mut guard = JobGuard::new(&env);
    let id = env.submit(&mut guard, &["submit-meta", "-r", LIVE_TEST_OBSID])[0];
    let id_str = id.to_string();

    let result = env.run(&["cancel", &id_str]);
    assert!(result.success, "{}", result.combined());
    guard.forget(id);

    let result = env.run(&["wait", &id_str]);
    assert!(!result.success, "{}", result.combined());
    assert!(
        result.combined().contains("has been cancelled"),
        "{}",
        result.combined()
    );
}

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_wait_on_an_unknown_job_fails() {
    let env = LiveEnv::new();

    let result = env.run(&["wait", LIVE_UNKNOWN_JOB_ID]);
    assert!(!result.success, "{}", result.combined());
    assert!(
        result
            .combined()
            .contains("wasn't found in your list of jobs"),
        "{}",
        result.combined()
    );
}

// ---------------------------------------------------------------------------
// Authentication
// ---------------------------------------------------------------------------

#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_an_invalid_api_key_is_rejected() {
    // A fresh HOME, or the shared session would be used and no login made.
    let env = LiveEnv::isolated();

    let mut cmd = env.command();
    cmd.env(ENV_API_KEY, INVALID_API_KEY).arg("list");
    let result = run(cmd);

    assert!(!result.success, "{}", result.combined());
    assert!(
        result.combined().contains(AUTH_FAILED_PREFIX),
        "{}",
        result.combined()
    );
}

/// A cached token the server did not issue is rejected with an auth error
/// code, and the client recovers by logging in again.
///
/// Runs at `-vv` to see the server's response bodies. Those include the
/// real tokens from the fresh login, so the output is never printed.
#[test]
#[ignore = "talks to a live MWA ASVO; run by hand, see the module docs"]
fn live_an_invalid_cached_token_is_replaced_by_a_fresh_login() {
    let env = LiveEnv::isolated();
    let fake_access = jwt_expiring_in(3600);
    write_session_at(&env.home, fake_access.clone(), jwt_expiring_in(86400));

    let result = env.run(&["list", "-vv"]);
    let output = result.combined();

    assert!(
        result.success,
        "list should recover with a fresh login (output withheld: it holds tokens)"
    );
    assert!(
        AUTH_ERROR_CODES.iter().any(|code| output.contains(code)),
        "the server should reject the cached token with one of {AUTH_ERROR_CODES:?} (output withheld: it holds tokens)"
    );
    let cache = std::fs::read_to_string(env.home.join(".mwa-asvo").join("tokens.json"))
        .expect("the fresh session should be cached");
    assert!(
        !cache.contains(&fake_access),
        "the fake token should have been replaced"
    );
}
