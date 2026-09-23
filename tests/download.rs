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
    env.server.mock(|when, then| {
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
