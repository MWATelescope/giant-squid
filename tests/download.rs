// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration tests for the download path, run against a local mock
//! server. Nothing is fetched from Acacia.
//!
//! The mock server serves the job's file as well as the API, so a real
//! download runs end to end: the bytes are fetched, written or untarred,
//! and the SHA1 checked, without Acacia being involved.
//!
//! Resume is not covered here; see the resume defects noted in
//! docs/TESTING.md, which need fixing before a test can pin the behaviour.

mod common;

use common::*;
use httpmock::prelude::*;
use indicatif::ProgressBar;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tempfile::TempDir;

use mwa_giant_squid::asvo::{AsvoClient, AsvoError, DownloadOptions};

/// The name the file is served under, and so the name it lands under:
/// the output path is the last segment of the download URL.
const DOWNLOAD_PATH: &str = "/downloads/1065880128_12345_meta.tar";
const DOWNLOAD_FILE: &str = "1065880128_12345_meta.tar";

/// A completed job whose `product` points at the mock server, in the shape
/// a real completed job uses.
fn ready_job_serving(url: &str, size: u64, sha1: &str) -> Value {
    let mut detail = job_detail(TEST_JOBID as i64, TEST_OBSID, "completed", 1);
    detail["product"] = json!({
        "files": [{ "type": "acacia", "url": url, "size": size, "sha1": sha1 }]
    });
    detail
}

fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A tar archive holding one small file, as a UTF-8 string so it can be
/// served as a body. Tar headers and ASCII contents are all valid UTF-8.
fn tar_containing(name: &str, contents: &str) -> String {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, name, contents.as_bytes())
        .expect("could not build the test tar");
    let bytes = builder.into_inner().expect("could not finish the test tar");
    String::from_utf8(bytes).expect("an ASCII tar should be valid UTF-8")
}

/// Download options writing into a temporary directory, with progress
/// output suppressed.
fn options<'a>(dir: &'a str, progress_bar: &'a ProgressBar) -> DownloadOptions<'a> {
    DownloadOptions {
        keep_tar: false,
        no_resume: false,
        hash: true,
        download_dir: dir,
        progress_bar,
        download_number: 1,
        download_count: 1,
    }
}

/// The `AsvoError` behind an `anyhow::Error` from a download call.
fn asvo_error(err: anyhow::Error) -> AsvoError {
    err.downcast::<AsvoError>()
        .expect("expected an AsvoError from the download path")
}

#[test]
fn downloading_an_unknown_job_id_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(
            TEST_JOBID,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    match asvo_error(err) {
        AsvoError::NoAsvoJob(jobid) => assert_eq!(jobid, TEST_JOBID),
        other => panic!("expected NoAsvoJob, got {other:?}"),
    }
}

#[test]
fn downloading_a_job_that_is_not_ready_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(TEST_JOBID as i64, TEST_OBSID, "queued", 1)]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(
            TEST_JOBID,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    match asvo_error(err) {
        AsvoError::NotReady { jobid, .. } => assert_eq!(jobid, TEST_JOBID),
        other => panic!("expected NotReady, got {other:?}"),
    }
}

/// A job with no `product` at all - which is every job that hasn't
/// completed, and any completed job the server describes differently than
/// expected - has no files to fetch.
#[test]
fn a_ready_job_reports_that_it_has_no_files() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(
        TEST_JOBID as i64,
        TEST_OBSID,
        "completed",
        1,
    )]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(
            TEST_JOBID,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    match asvo_error(err) {
        AsvoError::NoFiles(jobid) => assert_eq!(jobid, TEST_JOBID),
        other => panic!("expected NoFiles, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_dir(dir.path())
            .expect("the download directory should exist")
            .count(),
        0,
        "nothing should have been written"
    );
}

#[test]
fn downloading_an_unknown_obsid_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(1, "1061311664", "completed", 1)]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let obsid = TEST_OBSID.parse().expect("the test obsid should parse");
    let err = client
        .download_obsid(
            obsid,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    assert!(
        matches!(asvo_error(err), AsvoError::NoObsid(_)),
        "expected NoObsid"
    );
}

#[test]
fn an_obsid_whose_only_job_is_unfinished_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(
        TEST_JOBID as i64,
        TEST_OBSID,
        "staging",
        1,
    )]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let obsid = TEST_OBSID.parse().expect("the test obsid should parse");
    let err = client
        .download_obsid(
            obsid,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    assert!(
        matches!(asvo_error(err), AsvoError::NoJobReadyForObsid(_)),
        "expected NoJobReadyForObsid"
    );
}

#[test]
fn an_obsid_with_several_ready_jobs_is_ambiguous() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(1, TEST_OBSID, "completed", 1),
        job_detail(2, TEST_OBSID, "completed", 0),
    ]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let obsid = TEST_OBSID.parse().expect("the test obsid should parse");
    let err = client
        .download_obsid(
            obsid,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    assert!(
        matches!(asvo_error(err), AsvoError::TooManyObsids(_)),
        "expected TooManyObsids"
    );
}

/// A job whose `product` is present but empty is still not downloadable.
/// Guards against a future `product` mapping quietly producing an empty
/// file list and a silent success.
#[test]
fn a_job_listing_with_an_empty_product_is_not_downloadable() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOBID as i64, TEST_OBSID, "completed", 1);
    detail["product"] = json!({});
    env.mock_get_jobs(vec![detail]);
    let dir = TempDir::new().expect("could not create a download directory");
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(
            TEST_JOBID,
            &options(&dir.path().display().to_string(), &progress_bar),
        )
        .expect_err("expected the download to fail");

    assert!(
        matches!(asvo_error(err), AsvoError::NoFiles(_)),
        "expected NoFiles"
    );
}

// ---------------------------------------------------------------------------
// A download that succeeds
// ---------------------------------------------------------------------------

#[test]
fn a_ready_job_downloads_its_file_and_checks_the_hash() {
    let env = TestEnv::with_session();
    let payload = "giant-squid integration test payload";
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(payload);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        payload.len() as u64,
        &sha1_hex(payload.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("the download should succeed");

    assert_eq!(file.calls(), 1);
    let written =
        std::fs::read(dir.path().join(DOWNLOAD_FILE)).expect("the file should be written");
    assert_eq!(written, payload.as_bytes());
}

#[test]
fn a_downloaded_tar_is_unpacked_when_keep_tar_is_not_set() {
    let env = TestEnv::with_session();
    let contents = "METAFITS CONTENTS";
    let tar = tar_containing("1065880128.metafits", contents);
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(&tar);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        tar.len() as u64,
        &sha1_hex(tar.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &options(&dir_path, &progress_bar))
        .expect("the download should succeed");

    assert_eq!(file.calls(), 1);
    let unpacked = std::fs::read_to_string(dir.path().join("1065880128.metafits"))
        .expect("the archive contents should be unpacked");
    assert_eq!(unpacked, contents);
    assert!(
        !dir.path().join(DOWNLOAD_FILE).exists(),
        "the tar itself should not be kept"
    );
}

#[test]
fn a_hash_mismatch_is_reported() {
    let env = TestEnv::with_session();
    let payload = "giant-squid integration test payload";
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(payload);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        payload.len() as u64,
        "0000000000000000000000000000000000000000",
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(TEST_JOBID, &opts)
        .expect_err("the hash check should fail");

    match asvo_error(err) {
        AsvoError::HashMismatch {
            jobid,
            expected_hash,
            calculated_hash,
            ..
        } => {
            assert_eq!(jobid, TEST_JOBID);
            assert_eq!(expected_hash, "0000000000000000000000000000000000000000");
            assert_eq!(calculated_hash, sha1_hex(payload.as_bytes()));
        }
        other => panic!("expected HashMismatch, got {other:?}"),
    }
    // A hash mismatch is classed as transient, so it is retried under
    // exponential backoff in normal use. The harness sets
    // GIANT_SQUID_DOWNLOAD_RETRY_SECS=0 so the test doesn't sit in backoff.
    assert_eq!(file.calls(), 1, "retries are disabled in tests");
}

#[test]
fn an_expired_download_url_is_reported_as_gone() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(404).body("NoSuchKey");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        10,
        "0000000000000000000000000000000000000000",
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(TEST_JOBID, &opts)
        .expect_err("a 404 should fail the download");

    match asvo_error(err) {
        AsvoError::Http404Error { job_id } => assert_eq!(job_id, TEST_JOBID),
        other => panic!("expected Http404Error, got {other:?}"),
    }
}

/// A 403 (an expired signature, typically) is permanent, so it must fail
/// immediately rather than being retried with backoff.
#[test]
fn a_forbidden_download_fails_without_retrying() {
    let env = TestEnv::with_session();
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(403).body("SignatureDoesNotMatch");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        10,
        "0000000000000000000000000000000000000000",
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(TEST_JOBID, &opts)
        .expect_err("a 403 should fail the download");

    assert_eq!(file.calls(), 1, "a permanent error must not be retried");
    match asvo_error(err) {
        AsvoError::HttpError { status, .. } => assert_eq!(status, 403),
        other => panic!("expected HttpError, got {other:?}"),
    }
}

/// A file entry whose delivery type the client doesn't understand is
/// skipped, and a job left with nothing usable reports no files rather
/// than half-downloading.
#[test]
fn a_file_with_an_unknown_delivery_type_is_skipped() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOBID as i64, TEST_OBSID, "completed", 1);
    detail["product"] = json!({
        "files": [{ "type": "some_new_delivery", "url": "https://example.org/x.tar", "size": 1 }]
    });
    env.mock_get_jobs(vec![detail]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();

    let client = AsvoClient::new().expect("client should be created");
    let err = client
        .download_jobid(TEST_JOBID, &options(&dir_path, &progress_bar))
        .expect_err("expected the download to fail");

    assert!(
        matches!(asvo_error(err), AsvoError::NoFiles(_)),
        "expected NoFiles"
    );
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

/// The payload used by the resume tests, and where a partial file stops.
const RESUME_PAYLOAD: &str = "giant-squid integration test payload";
const RESUME_SPLIT: usize = 12;

#[test]
fn a_partial_file_is_resumed_from_where_it_stopped() {
    let env = TestEnv::with_session();
    let (head, tail) = RESUME_PAYLOAD.split_at(RESUME_SPLIT);
    let file = env.server.mock(|when, then| {
        when.method(GET)
            .path(DOWNLOAD_PATH)
            .header("range", format!("bytes={RESUME_SPLIT}-").as_str());
        then.status(206).body(tail);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), head).expect("could not seed a partial file");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("the resumed download should succeed");

    assert_eq!(file.calls(), 1, "the range request should be made once");
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should still be there");
    // Also proves the hash was checked against the assembled file: the
    // expected hash covers the whole payload, but only the tail was fetched.
    assert_eq!(written, RESUME_PAYLOAD);
}

#[test]
fn a_complete_and_verified_file_is_not_fetched_again() {
    let env = TestEnv::with_session();
    let requests = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(500).body("should not have been asked for");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), RESUME_PAYLOAD)
        .expect("could not seed a complete file");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("an already-complete file should be a no-op");

    assert_eq!(requests.calls(), 0, "nothing should have been fetched");
}

#[test]
fn a_complete_file_with_the_wrong_contents_is_fetched_again() {
    let env = TestEnv::with_session();
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(RESUME_PAYLOAD);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    // Right length, wrong bytes: the size check passes but the hash won't.
    let corrupt = "X".repeat(RESUME_PAYLOAD.len());
    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), &corrupt)
        .expect("could not seed a corrupt file");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("the download should restart and succeed");

    assert_eq!(file.calls(), 1);
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should be rewritten");
    // Truncated rather than appended to, so no doubled length.
    assert_eq!(written, RESUME_PAYLOAD);
}

#[test]
fn a_partial_file_is_left_alone_when_no_resume_is_set() {
    let env = TestEnv::with_session();
    let requests = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(500).body("should not have been asked for");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let (head, _) = RESUME_PAYLOAD.split_at(RESUME_SPLIT);
    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), head).expect("could not seed a partial file");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;
    opts.no_resume = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("--no-resume should skip the file, not fail");

    assert_eq!(requests.calls(), 0, "nothing should have been fetched");
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the partial file should still be there");
    assert_eq!(written, head, "the partial file should be untouched");
}

/// A server may ignore a range request and answer 200 with the whole file.
/// Appending that to a partial file would corrupt it, so the download has to
/// start again.
#[test]
fn a_server_that_ignores_the_range_request_restarts_the_download() {
    let env = TestEnv::with_session();
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(RESUME_PAYLOAD);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let (head, _) = RESUME_PAYLOAD.split_at(RESUME_SPLIT);
    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), head).expect("could not seed a partial file");
    let dir_path = dir.path().display().to_string();
    let progress_bar = ProgressBar::hidden();
    let mut opts = options(&dir_path, &progress_bar);
    opts.keep_tar = true;

    let client = AsvoClient::new().expect("client should be created");
    client
        .download_jobid(TEST_JOBID, &opts)
        .expect("the restarted download should succeed");

    assert_eq!(file.calls(), 1);
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should be rewritten");
    assert_eq!(
        written, RESUME_PAYLOAD,
        "the partial bytes must not be left in front of the full file"
    );
}
