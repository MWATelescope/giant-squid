// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Replays a recording captured from a live MWA ASVO.
//!
//! `tests/fixtures/login_and_get_jobs.yaml` was recorded against
//! test-asvo by `tests/record.rs` and scrubbed by
//! `tools/scrub_recording.py`. Loading it into a mock server turns the
//! recorded requests into matching criteria, so this test asserts the
//! client's behaviour against a real server response rather than one this
//! repo invented.
//!
//! The fixture also holds the login exchange, but this test writes a cached
//! session instead of replaying it. The recorded login request body carries
//! the client version (`giant-squidv3.0.0`), so a version bump would stop it
//! matching and break the test for an unrelated reason. It is kept in the
//! fixture for reference and for manual use.
//!
//! See docs/TESTING.md.

mod common;

use common::*;

use mwa_giant_squid::asvo::{AsvoClient, AsvoJobState, AsvoJobType, Delivery};

/// Values from the recorded response, so a re-record that changes them
/// fails loudly here rather than silently weakening the test.
const RECORDED_JOB_ID: u32 = 30000517;
const RECORDED_OBSID: u64 = 1115977528;
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

    let client = AsvoClient::new().expect("client should be created");
    // The recording was made with --days 30, and the recorded request is
    // the matching criteria, so the same argument is required here.
    let jobs = client
        .get_jobs(Some(30))
        .expect("the recorded listing should be served");

    assert_eq!(jobs.0.len(), 1);
    let job = &jobs.0[0];
    assert_eq!(job.jobid, RECORDED_JOB_ID);
    assert_eq!(job.obsid.get(), RECORDED_OBSID);
    assert_eq!(job.jtype, AsvoJobType::DownloadMetadata);
    assert_eq!(job.state, AsvoJobState::Ready);
    assert!(job.completed.is_some(), "completed should be parsed");
}

/// The point of the recording: `product` is a free-form object in the
/// schema, so this pins the mapping against a real payload.
#[test]
fn a_recorded_jobs_product_becomes_a_file_list() {
    let env = TestEnv::with_session();
    env.server.playback(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("login_and_get_jobs.yaml"),
    );

    let client = AsvoClient::new().expect("client should be created");
    let jobs = client
        .get_jobs(Some(30))
        .expect("the recorded listing should be served");

    let files = jobs.0[0]
        .files
        .as_ref()
        .expect("a completed job should carry its files");
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.r#type, Delivery::Acacia);
    assert_eq!(file.size, RECORDED_SIZE);
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
