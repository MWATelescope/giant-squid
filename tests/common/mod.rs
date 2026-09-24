// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Harness for `tests/cli.rs`, which runs the built binary as a subprocess.
//!
//! These tests must stay in `tests/`: Cargo sets `CARGO_BIN_EXE_giant-squid`
//! only for integration tests. The mock-server helpers they share with the
//! unit tests live in `src/test_common.rs` and are pulled in below.

#![allow(dead_code)]

#[path = "../../src/test_common.rs"]
mod shared;

pub use shared::*;

use std::process::Command;

use httpmock::prelude::*;
use httpmock::Mock;
use serde_json::{json, Value};
use tempfile::TempDir;

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
            .env("GIANT_SQUID_DOWNLOAD_RETRY_SECS", "0")
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
