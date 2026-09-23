// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Captures fixtures from a live MWA ASVO, for replay by the offline tests.
//!
//! This is `#[ignore]`d: CI never runs it, and it is the only test in the
//! suite that talks to a real server. Run it by hand when the API changes:
//!
//! ```text
//! HOME=$(mktemp -d) \
//! MWA_ASVO_API_KEY=<your key> \
//! MWA_ASVO_RECORD_TARGET=https://test-asvo.mwatelescope.org \
//!   cargo test --test record -- --ignored --nocapture
//! ```
//!
//! A throwaway `HOME` is deliberate: it forces a fresh login, so the login
//! exchange is captured too, and it leaves the real token cache (shared with
//! mwa-cli) untouched.
//!
//! The recording it writes contains real JWTs, your user ID, login name and
//! email. Scrub it before committing:
//!
//! ```text
//! python3 tools/scrub_recording.py <recorded file> tests/fixtures/<name>.yaml
//! ```
//!
//! Only read-only endpoints are recorded. Recording a submission would
//! create a real job on the target server, so that is deliberately not
//! automated here.

use httpmock::prelude::*;

use mwa_giant_squid::asvo::AsvoClient;

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
    println!("scrub it before committing - see the module docs in tests/record.rs");
}
