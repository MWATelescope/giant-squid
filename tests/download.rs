// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Integration tests for the download path, run against a local mock
//! server. Nothing is fetched from Acacia.
//!
//! Coverage here stops short of a successful download, because apiv2 does
//! not populate `AsvoJob.files` yet: `job_detail_to_asvo_job` sets it to
//! `None` because the `product` field's shape is undocumented (the schema
//! types it as a free-form object). Every download therefore ends in
//! `AsvoError::NoFiles`, which `a_ready_job_reports_that_it_has_no_files`
//! pins. Once `product` is mapped, the happy path, hash verification, tar
//! handling and resume can be tested the same way, by serving the file
//! from this mock server. See docs/TESTING.md.

mod common;

use common::*;
use indicatif::ProgressBar;
use serde_json::json;
use tempfile::TempDir;

use mwa_giant_squid::asvo::{AsvoClient, AsvoError, DownloadOptions};

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
    env.mock_get_jobs(vec![job_detail(
        TEST_JOBID as i64,
        TEST_OBSID,
        "queued",
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
        AsvoError::NotReady { jobid, .. } => assert_eq!(jobid, TEST_JOBID),
        other => panic!("expected NotReady, got {other:?}"),
    }
}

/// A ready job still cannot be downloaded, because apiv2 never fills in
/// `files` - see the module docs. When `product` is mapped, this test
/// should be replaced by one that downloads successfully.
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
