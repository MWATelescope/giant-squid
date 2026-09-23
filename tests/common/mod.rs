// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared harness for the integration tests.
//!
//! Every test here runs against a local [`MockServer`], never a real MWA
//! ASVO. Two things make that safe and repeatable:
//!
//! 1. `MWA_ASVO_HOST` points the client at the mock server, and `HOME`
//!    points the token cache at a temporary directory, so a developer's
//!    real `~/.mwa-asvo/tokens.json` is never read or written.
//! 2. Those variables are process-wide, and cargo runs tests in parallel
//!    threads, so [`TestEnv`] holds a lock for the life of the test. Tests
//!    using it therefore run one at a time.
//!
//! See docs/TESTING.md for the wider plan.

#![allow(dead_code)]

use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use httpmock::prelude::*;
use httpmock::Mock;
use serde_json::{json, Value};
use tempfile::TempDir;

/// Serialises access to the environment variables the client reads.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Values the harness uses in place of real credentials and user details.
pub const TEST_API_KEY: &str = "not-a-real-api-key";
pub const TEST_USER_ID: i64 = 4242;
pub const TEST_USER_LOGIN: &str = "test_user";
pub const TEST_USER_EMAIL: &str = "test_user@example.org";

/// An obsid and job ID used across the tests.
pub const TEST_OBSID: &str = "1065880128";
pub const TEST_OBSID_I64: i64 = 1065880128;
pub const TEST_JOBID: u32 = 12345;

/// The environment variables the harness overrides.
const MANAGED_VARS: [&str; 4] = [
    "MWA_ASVO_HOST",
    "MWA_ASVO_API_KEY",
    "MWA_ASVO_API_TIMEOUT",
    "HOME",
];

/// A mock MWA ASVO, with the environment pointed at it.
pub struct TestEnv {
    pub server: MockServer,
    home: TempDir,
    saved: Vec<(&'static str, Option<String>)>,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    /// Start a mock server with a valid cached session already on disk, so
    /// `AsvoClient::new()` uses it instead of logging in. This is the usual
    /// starting point: it keeps tests focused on the endpoint under test.
    pub fn with_session() -> Self {
        let env = Self::bare();
        env.write_session(jwt_expiring_in(3600), jwt_expiring_in(86400));
        env
    }

    /// Start a mock server with no cached session, so `AsvoClient::new()`
    /// has to log in. Used by the authentication tests.
    pub fn without_session() -> Self {
        Self::bare()
    }

    /// As [`Self::without_session`], but with no API key either.
    pub fn without_api_key() -> Self {
        let env = Self::bare();
        clear_env("MWA_ASVO_API_KEY");
        env
    }

    fn bare() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = MANAGED_VARS
            .iter()
            .map(|var| (*var, std::env::var(var).ok()))
            .collect();

        let server = MockServer::start();
        let home = TempDir::new().expect("could not create a temporary HOME");

        set_env("MWA_ASVO_HOST", &server.base_url());
        set_env("MWA_ASVO_API_KEY", TEST_API_KEY);
        set_env("MWA_ASVO_API_TIMEOUT", "5");
        set_env("HOME", &home.path().display().to_string());

        Self {
            server,
            home,
            saved,
            _guard: guard,
        }
    }

    /// Write a cached session to the temporary `HOME`, with the supplied
    /// tokens. The stored expiry timestamps are taken from each token's own
    /// `exp` claim, so the file is self-consistent.
    pub fn write_session(&self, access_token: String, refresh_token: String) {
        write_session_at(self.home.path(), access_token, refresh_token);
    }

    /// The cached session as it stands on disk, or `None` if there isn't
    /// one. Lets a test check that a login or refresh was persisted.
    pub fn cached_session(&self) -> Option<Value> {
        let path = self.home.path().join(".mwa-asvo").join("tokens.json");
        let contents = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Serve `POST /api/v2/api_login` with a successful login response.
    pub fn mock_login(&self) -> Mock<'_> {
        self.server.mock(|when, then| {
            when.method(POST).path("/api/v2/api_login");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(login_response());
        })
    }

    /// Serve `POST /api/v2/api_login` with a failure.
    pub fn mock_login_failure(&self, status: u16, body: &str) -> Mock<'_> {
        self.server.mock(|when, then| {
            when.method(POST).path("/api/v2/api_login");
            then.status(status).body(body);
        })
    }

    /// Serve `POST /api/v2/get_jobs` with one page of jobs.
    pub fn mock_get_jobs(&self, jobs: Vec<Value>) -> Mock<'_> {
        let total_count = jobs.len();
        self.server.mock(|when, then| {
            when.method(POST).path("/api/v2/get_jobs");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "jobs": jobs, "total_count": total_count }));
        })
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (var, value) in &self.saved {
            match value {
                Some(value) => set_env(var, value),
                None => clear_env(var),
            }
        }
    }
}

/// A mock MWA ASVO for tests that run the built binary as a subprocess.
///
/// Unlike [`TestEnv`] this touches no process-wide state: the child's
/// environment is set on the [`Command`] itself, so these tests run in
/// parallel with each other and with everything else.
pub struct CliEnv {
    pub server: MockServer,
    home: TempDir,
}

impl CliEnv {
    /// Start a mock server and a temporary `HOME` holding a valid cached
    /// session, so the binary does not need to log in.
    pub fn with_session() -> Self {
        let server = MockServer::start();
        let home = TempDir::new().expect("could not create a temporary HOME");
        write_session_at(home.path(), jwt_expiring_in(3600), jwt_expiring_in(86400));
        Self { server, home }
    }

    /// The built `giant-squid` binary, pointed at the mock server.
    ///
    /// Every variable the client reads is set or cleared explicitly, so a
    /// developer's own `GIANT_SQUID_*` or `MWA_ASVO_*` settings cannot leak
    /// into a test run.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_giant-squid"));
        cmd.env("MWA_ASVO_HOST", self.server.base_url())
            .env("MWA_ASVO_API_KEY", TEST_API_KEY)
            .env("HOME", self.home.path())
            .env_remove("MWA_ASVO_API_TIMEOUT")
            .env_remove("GIANT_SQUID_DELIVERY")
            .env_remove("GIANT_SQUID_DELIVERY_FORMAT")
            .env_remove("GIANT_SQUID_BUF_SIZE");
        cmd
    }

    /// Serve `POST /api/v2/get_jobs` with one page of jobs.
    pub fn mock_get_jobs(&self, jobs: Vec<Value>) -> Mock<'_> {
        let total_count = jobs.len();
        self.server.mock(|when, then| {
            when.method(POST).path("/api/v2/get_jobs");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "jobs": jobs, "total_count": total_count }));
        })
    }
}

/// What a finished subprocess run produced.
pub struct Run {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// Everything the run printed, whichever stream it went to. Log output
    /// and clap's diagnostics land on different streams, so assertions on
    /// messages use this rather than picking one.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// The first JSON object printed on stdout. Lets a test read `--json`
    /// output without tripping over interleaved log lines.
    pub fn stdout_json(&self) -> Value {
        let line = self
            .stdout
            .lines()
            .find(|l| l.trim_start().starts_with('{'))
            .expect("expected a JSON object on stdout");
        serde_json::from_str(line.trim()).expect("stdout JSON should parse")
    }
}

/// Run a command to completion and capture its output.
pub fn run(mut cmd: Command) -> Run {
    let out = cmd.output().expect("could not run the giant-squid binary");
    Run {
        success: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Write a token cache under `home`, as the client expects to find it.
fn write_session_at(home: &std::path::Path, access_token: String, refresh_token: String) {
    let dir = home.join(".mwa-asvo");
    std::fs::create_dir_all(&dir).expect("could not create the token cache directory");
    let tokens = json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "access_expires_at": jwt_expiry(&access_token).to_rfc3339(),
        "refresh_expires_at": jwt_expiry(&refresh_token).to_rfc3339(),
        "user_id": TEST_USER_ID,
        "user_login": TEST_USER_LOGIN,
        "user_email": TEST_USER_EMAIL,
    });
    std::fs::write(
        dir.join("tokens.json"),
        serde_json::to_string_pretty(&tokens).unwrap(),
    )
    .expect("could not write the token cache");
}

/// A JWT whose payload carries an `exp` claim `seconds` from now.
///
/// The client only base64-decodes the payload to read `exp` - it never
/// verifies the signature - so the header and signature segments are
/// placeholders.
pub fn jwt_expiring_in(seconds: i64) -> String {
    let exp = (Utc::now() + Duration::seconds(seconds)).timestamp();
    let payload = URL_SAFE_NO_PAD.encode(format!(r#"{{"exp":{exp}}}"#));
    format!("notaheader.{payload}.notasignature")
}

/// The expiry encoded in a token built by [`jwt_expiring_in`].
fn jwt_expiry(token: &str) -> chrono::DateTime<Utc> {
    let payload = token.split('.').nth(1).expect("malformed test JWT");
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .expect("test JWT payload should decode");
    let claim: Value = serde_json::from_slice(&decoded).expect("test JWT payload should be JSON");
    let exp = claim["exp"].as_i64().expect("test JWT should carry exp");
    chrono::DateTime::from_timestamp(exp, 0).expect("test JWT exp should be in range")
}

/// A successful `ApiLoginResponse` body.
pub fn login_response() -> Value {
    json!({
        "access_token": jwt_expiring_in(3600),
        "refresh_token": jwt_expiring_in(86400),
        "token_type": "bearer",
        "user": {
            "email": TEST_USER_EMAIL,
            "id": TEST_USER_ID,
            "is_superuser": false,
            "login": TEST_USER_LOGIN,
        }
    })
}

/// A successful `TokenResponse` body, as returned by the refresh endpoint.
pub fn token_response() -> Value {
    json!({
        "access_token": jwt_expiring_in(3600),
        "refresh_token": jwt_expiring_in(86400),
        "token_type": "bearer",
    })
}

/// A structured `ErrorResponse` body.
pub fn error_response(error_code: &str, message: &str) -> Value {
    json!({
        "error_code": error_code,
        "message": message,
        "detail": "detail from the mock server",
        "suggestion": "try something else",
    })
}

/// A `JobSubmittedResponse` body.
pub fn job_submitted_response(job_id: u64) -> Value {
    json!({
        "job_id": job_id,
        "message": "Job submitted",
        "status": "success",
    })
}

/// One entry of a `get_jobs` page.
///
/// The timestamps are deliberately naive (no `Z`) and `modified` is
/// omitted entirely, which is what the real server sends - the client
/// normalises both before deserialising.
pub fn job_detail(id: i64, obs_id: &str, job_state: &str, job_type: i64) -> Value {
    json!({
        "created": "2026-09-08T05:41:54.757232",
        "first_name": "Test",
        "id": id,
        "job_params": { "obs_id": obs_id, "delivery": "acacia" },
        "job_state": job_state,
        "job_type": job_type,
        "last_name": "User",
        "user_id": TEST_USER_ID,
    })
}

/// Set an environment variable for the duration of a test.
///
/// Sound because [`TestEnv`] holds `ENV_LOCK` for the life of the test, so
/// no other test reads or writes these variables concurrently.
fn set_env(key: &str, value: &str) {
    std::env::set_var(key, value);
}

fn clear_env(key: &str) {
    std::env::remove_var(key);
}
