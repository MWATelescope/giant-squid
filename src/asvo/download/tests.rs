// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the download path, run against a local mock
//! server. Nothing is fetched from Acacia.
//!
//! The mock server serves the job's file as well as the API, so a real
//! download runs end to end: the bytes are fetched, written or untarred,
//! and the SHA1 checked, without Acacia being involved.
//!
//! Resume is covered here, for both forms of download:
//!
//! - `--keep-tar`: a partial tar file is resumed with a range request, and a
//!   complete file that matches the hash is skipped (section "Resume").
//! - Stream-untar: a retry carries on from the failed attempt (`retries`),
//!   a new run carries on from the files that an earlier run wrote
//!   (`reruns`), and the resume file of an earlier run is used when it is
//!   still valid (`sidecar`).
//!
//! The `unsafe_paths` module covers tar entries whose path is not inside
//! the download directory.

use httpmock::prelude::*;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tempfile::TempDir;

use crate::asvo::{
    AsvoApiError, AsvoClient, AsvoError, DownloadOptions, DownloadProgress,
    DEFAULT_DOWNLOAD_BUFFER_SIZE,
};
use crate::test_common::*;
use crate::test_config::client_config;

/// The name the file is served under, and so the name it lands under:
/// the output path is the last segment of the download URL.
const DOWNLOAD_PATH: &str = "/downloads/1065880128_12345_meta.tar";
const DOWNLOAD_FILE: &str = "1065880128_12345_meta.tar";

/// A completed job whose `product` points at the mock server, in the shape
/// a real completed job uses.
fn ready_job_serving(url: &str, size: u64, sha1: &str) -> Value {
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    detail["product"] = json!({
        "files": [{ "type": "acacia", "url": url, "size": size, "sha1": sha1 }]
    });
    detail
}

fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    crate::helpers::to_hex(&hasher.finalize())
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

/// Download options writing into a temporary directory, with no progress
/// callback and retries disabled.
///
/// Retries are disabled because a test that deliberately triggers a
/// transient download failure would otherwise retry under exponential
/// backoff for fifteen minutes.
fn options(dir: &str) -> DownloadOptions<'_> {
    DownloadOptions {
        keep_tar: false,
        no_resume: false,
        hash: true,
        download_dir: dir,
        progress: None,
        download_number: 1,
        download_count: 1,
        buffer_size: DEFAULT_DOWNLOAD_BUFFER_SIZE,
        retry_duration: std::time::Duration::ZERO,
        should_stop: None,
    }
}

#[test]
fn downloading_an_unknown_job_id_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(
            crate::test_config::TEST_ASVO_JOB_ID,
            &options(&dir.path().display().to_string()),
        )
        .expect_err("expected the download to fail");

    match err {
        AsvoError::NoAsvoJob(job_id) => assert_eq!(job_id, crate::test_config::TEST_ASVO_JOB_ID),
        other => panic!("expected NoAsvoJob, got {other:?}"),
    }
}

#[test]
fn a_failed_job_listing_is_reported_as_an_api_error() {
    let env = TestEnv::with_session();
    env.server.mock(|when, then| {
        when.method(POST).path("/api/v2/get_jobs");
        then.status(500)
            .header("content-type", "application/json")
            .json_body(error_response("INTERNAL_ERROR", "the job list failed"));
    });
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(
            crate::test_config::TEST_ASVO_JOB_ID,
            &options(&dir.path().display().to_string()),
        )
        .expect_err("expected the download to fail");

    assert!(
        matches!(err, AsvoError::AsvoApi(_)),
        "expected AsvoApi, got {err:?}"
    );
}

#[test]
fn downloading_a_job_that_is_not_ready_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(
        TEST_JOB_ID as i64,
        TEST_OBS_ID,
        "queued",
        1,
    )]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(
            crate::test_config::TEST_ASVO_JOB_ID,
            &options(&dir.path().display().to_string()),
        )
        .expect_err("expected the download to fail");

    match err {
        AsvoError::NotReady { job_id, .. } => {
            assert_eq!(job_id, crate::test_config::TEST_ASVO_JOB_ID)
        }
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
        TEST_JOB_ID as i64,
        TEST_OBS_ID,
        "completed",
        1,
    )]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(
            crate::test_config::TEST_ASVO_JOB_ID,
            &options(&dir.path().display().to_string()),
        )
        .expect_err("expected the download to fail");

    match err {
        AsvoError::NoFiles(job_id) => assert_eq!(job_id, crate::test_config::TEST_ASVO_JOB_ID),
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
fn downloading_an_unknown_obs_id_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(1, "1061311664", "completed", 1)]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let obs_id = TEST_OBS_ID.parse().expect("the test obsid should parse");
    let err = client
        .download_obs(obs_id, &options(&dir.path().display().to_string()))
        .expect_err("expected the download to fail");

    assert!(matches!(err, AsvoError::NoObsId(_)), "expected NoObsid");
}

#[test]
fn an_obs_id_whose_only_job_is_unfinished_is_reported() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(
        TEST_JOB_ID as i64,
        TEST_OBS_ID,
        "staging",
        1,
    )]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let obs_id = TEST_OBS_ID.parse().expect("the test obsid should parse");
    let err = client
        .download_obs(obs_id, &options(&dir.path().display().to_string()))
        .expect_err("expected the download to fail");

    assert!(
        matches!(err, AsvoError::NoJobReadyForObsId(_)),
        "expected NoJobReadyForObsid"
    );
}

#[test]
fn an_obs_id_with_several_ready_jobs_is_ambiguous() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![
        job_detail(1, TEST_OBS_ID, "completed", 1),
        job_detail(2, TEST_OBS_ID, "completed", 0),
    ]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let obs_id = TEST_OBS_ID.parse().expect("the test obsid should parse");
    let err = client
        .download_obs(obs_id, &options(&dir.path().display().to_string()))
        .expect_err("expected the download to fail");

    assert!(
        matches!(err, AsvoError::TooManyObsIds(_)),
        "expected TooManyObsids"
    );
}

/// A job whose `product` is present but empty is still not downloadable.
/// Guards against a future `product` mapping quietly producing an empty
/// file list and a silent success.
#[test]
fn a_job_listing_with_an_empty_product_is_not_downloadable() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    detail["product"] = json!({});
    env.mock_get_jobs(vec![detail]);
    let dir = TempDir::new().expect("could not create a download directory");

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(
            crate::test_config::TEST_ASVO_JOB_ID,
            &options(&dir.path().display().to_string()),
        )
        .expect_err("expected the download to fail");

    assert!(matches!(err, AsvoError::NoFiles(_)), "expected NoFiles");
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should succeed");

    assert_eq!(file.calls(), 1);
    let written =
        std::fs::read(dir.path().join(DOWNLOAD_FILE)).expect("the file should be written");
    assert_eq!(written, payload.as_bytes());
}

#[test]
fn a_download_reports_its_progress_to_the_callback() {
    let env = TestEnv::with_session();
    let payload = "giant-squid integration test payload";
    env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(payload);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        payload.len() as u64,
        &sha1_hex(payload.as_bytes()),
    )]);

    let events = std::sync::Mutex::new(Vec::new());
    let progress = |event: DownloadProgress| events.lock().unwrap().push(event);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.progress = Some(&progress);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should succeed");

    let events = events.into_inner().unwrap();
    match events.first() {
        Some(DownloadProgress::Started {
            job_id,
            total_bytes,
            position,
            ..
        }) => {
            assert_eq!(*job_id, crate::test_config::TEST_ASVO_JOB_ID);
            assert_eq!(*total_bytes, payload.len() as u64);
            assert_eq!(*position, 0);
        }
        other => panic!("expected Started first, got {other:?}"),
    }
    assert_eq!(events.last(), Some(&DownloadProgress::Finished));
    let advanced: u64 = events
        .iter()
        .map(|e| match e {
            DownloadProgress::Advanced { bytes } => *bytes,
            _ => 0,
        })
        .sum();
    assert_eq!(
        advanced,
        payload.len() as u64,
        "every byte should be reported"
    );
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

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &options(&dir_path))
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("the hash check should fail");

    match err {
        AsvoError::HashMismatch {
            job_id,
            expected_hash,
            calculated_hash,
            ..
        } => {
            assert_eq!(job_id, crate::test_config::TEST_ASVO_JOB_ID);
            assert_eq!(expected_hash, "0000000000000000000000000000000000000000");
            assert_eq!(calculated_hash, sha1_hex(payload.as_bytes()));
        }
        other => panic!("expected HashMismatch, got {other:?}"),
    }
    // A hash mismatch is classed as transient, so it is retried under
    // exponential backoff in normal use. `options` sets a zero
    // retry_duration so the test doesn't sit in backoff.
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("a 404 should fail the download");

    match err {
        AsvoError::Http404Error { job_id } => {
            assert_eq!(job_id, crate::test_config::TEST_ASVO_JOB_ID)
        }
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("a 403 should fail the download");

    assert_eq!(file.calls(), 1, "a permanent error must not be retried");
    match err {
        AsvoError::HttpError { status, .. } => assert_eq!(status, 403),
        other => panic!("expected HttpError, got {other:?}"),
    }
}

/// Since schema 1.13 a file's `type` is `acacia`, `scratch` or `dug`. A type
/// that the schema does not list fails the listing, and so the download
/// (it does not skip the file). The user chose this over skipping the file.
#[test]
fn a_file_type_the_schema_does_not_list_fails_the_download() {
    let env = TestEnv::with_session();
    let mut detail = job_detail(TEST_JOB_ID as i64, TEST_OBS_ID, "completed", 1);
    detail["product"] = json!({
        "files": [{ "type": "some_new_delivery", "url": "https://example.org/x.tar", "size": 1 }]
    });
    env.mock_get_jobs(vec![detail]);

    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &options(&dir_path))
        .expect_err("expected the download to fail");

    assert!(
        matches!(&err, AsvoError::AsvoApi(AsvoApiError::BadJson(_))),
        "expected a decode error, got {err:?}"
    );
    assert!(err.to_string().contains("some_new_delivery"), "{err}");
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

/// A `--keep-tar` download refuses to write through a symbolic link that is
/// already at the place of the tar file, even one that points nowhere.
#[cfg(unix)]
#[test]
fn a_keep_tar_download_refuses_a_symlink() {
    let env = TestEnv::with_session();
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body("payload");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        7,
        &sha1_hex(b"payload"),
    )]);
    let dir = TempDir::new().expect("could not create a download directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let target = elsewhere.path().join("victim.tar");
    std::os::unix::fs::symlink(&target, dir.path().join(DOWNLOAD_FILE))
        .expect("could not make the link");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("the link should be refused");

    assert!(
        matches!(err, AsvoError::SymlinkInDownloadDir { .. }),
        "{err:?}"
    );
    assert!(!target.exists(), "nothing may be written through the link");
    assert_eq!(file.calls(), 0, "nothing should be fetched");
}

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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the resumed download should succeed");

    assert_eq!(file.calls(), 1, "the range request should be made once");
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should still be there");
    // Also proves the hash was checked against the assembled file: the
    // expected hash covers the whole payload, but only the tail was fetched.
    assert_eq!(written, RESUME_PAYLOAD);
}

/// A resumed `--keep-tar` file joins bytes from two runs, so it is checked
/// even without a hash check. The partial file has the wrong contents, so
/// only a check of the assembled file can find the error.
#[test]
fn without_a_hash_check_a_resumed_keep_tar_file_is_still_checked() {
    let env = TestEnv::with_session();
    let (head, tail) = RESUME_PAYLOAD.split_at(RESUME_SPLIT);
    env.server.mock(|when, then| {
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
    std::fs::write(dir.path().join(DOWNLOAD_FILE), "x".repeat(head.len()))
        .expect("could not seed a partial file");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.hash = false;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("the wrong partial file should fail the hash check");

    assert!(matches!(err, AsvoError::HashMismatch { .. }), "got {err:?}");
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should restart and succeed");

    assert_eq!(file.calls(), 1);
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should be rewritten");
    // Truncated rather than appended to, so no doubled length.
    assert_eq!(written, RESUME_PAYLOAD);
}

/// Seed `contents` as the keep-tar output file, then download with
/// `--no-resume`, from a server that sends the whole file only when it is
/// asked for without a range. Returns the number of whole-file requests and
/// what is on disk afterwards.
fn keep_tar_no_resume_download(contents: &[u8]) -> (usize, String) {
    let env = TestEnv::with_session();
    let whole = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH).header_missing("range");
        then.status(200).body(RESUME_PAYLOAD);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), contents).expect("could not seed a file");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.no_resume = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should succeed");

    let written =
        std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE)).expect("the file should be there");
    (whole.calls(), written)
}

#[test]
fn a_partial_file_is_downloaded_again_when_no_resume_is_set() {
    let (head, _) = RESUME_PAYLOAD.split_at(RESUME_SPLIT);

    let (whole_requests, written) = keep_tar_no_resume_download(head.as_bytes());

    assert_eq!(
        whole_requests, 1,
        "the whole file should be fetched, without a range"
    );
    assert_eq!(written, RESUME_PAYLOAD);
}

#[test]
fn a_complete_and_verified_file_is_skipped_when_no_resume_is_set() {
    let (whole_requests, written) = keep_tar_no_resume_download(RESUME_PAYLOAD.as_bytes());

    assert_eq!(whole_requests, 0, "nothing should have been fetched");
    assert_eq!(written, RESUME_PAYLOAD);
}

#[test]
fn a_complete_file_with_the_wrong_contents_is_downloaded_again_when_no_resume_is_set() {
    let wrong = "x".repeat(RESUME_PAYLOAD.len());

    let (whole_requests, written) = keep_tar_no_resume_download(wrong.as_bytes());

    assert_eq!(whole_requests, 1);
    assert_eq!(written, RESUME_PAYLOAD);
}

#[test]
fn a_file_larger_than_the_download_is_downloaded_again() {
    let env = TestEnv::with_session();
    let whole = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH).header_missing("range");
        then.status(200).body(RESUME_PAYLOAD);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        RESUME_PAYLOAD.len() as u64,
        &sha1_hex(RESUME_PAYLOAD.as_bytes()),
    )]);

    let dir = TempDir::new().expect("could not create a download directory");
    let larger = format!("{RESUME_PAYLOAD} and more");
    std::fs::write(dir.path().join(DOWNLOAD_FILE), larger).expect("could not seed a file");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should succeed");

    assert_eq!(whole.calls(), 1);
    let written =
        std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE)).expect("the file should be there");
    assert_eq!(
        written, RESUME_PAYLOAD,
        "the file should be replaced, not appended to"
    );
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
    let mut opts = options(&dir_path);
    opts.keep_tar = true;

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the restarted download should succeed");

    assert_eq!(file.calls(), 1);
    let written = std::fs::read_to_string(dir.path().join(DOWNLOAD_FILE))
        .expect("the file should be rewritten");
    assert_eq!(
        written, RESUME_PAYLOAD,
        "the partial bytes must not be left in front of the full file"
    );
}

// ---------------------------------------------------------------------------
// Stopping a download (DownloadOptions::should_stop)
// ---------------------------------------------------------------------------

/// A download stopped before its first chunk ends with `Interrupted`.
#[test]
fn a_download_stops_when_the_caller_asks() {
    let env = TestEnv::with_session();
    let payload = "giant-squid integration test payload";
    env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(payload);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        payload.len() as u64,
        &sha1_hex(payload.as_bytes()),
    )]);

    let asked = std::sync::atomic::AtomicUsize::new(0);
    let stop = || {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        true
    };
    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.should_stop = Some(&stop);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("the download should stop");

    assert!(matches!(err, AsvoError::Interrupted), "got {err:?}");
    assert_eq!(
        asked.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a stop is not retried"
    );
}

/// A download that the caller lets run is not affected by the hook.
#[test]
fn a_download_that_is_not_stopped_completes() {
    let env = TestEnv::with_session();
    let payload = "giant-squid integration test payload";
    env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(200).body(payload);
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        payload.len() as u64,
        &sha1_hex(payload.as_bytes()),
    )]);

    let asked = std::sync::atomic::AtomicUsize::new(0);
    let stop = || {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        false
    };
    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.should_stop = Some(&stop);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect("the download should succeed");

    assert!(asked.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    let written =
        std::fs::read(dir.path().join(DOWNLOAD_FILE)).expect("the file should be written");
    assert_eq!(written, payload.as_bytes());
}

/// A stop during the wait before a retry is seen within a short time, not
/// after the back-off interval.
#[test]
fn a_stop_ends_the_wait_before_a_retry() {
    /// Long enough that, without the stop, the test would wait many
    /// back-off intervals.
    const LONG_RETRY: std::time::Duration = std::time::Duration::from_secs(300);
    /// Far more than a stop takes, far less than LONG_RETRY.
    const MAX_STOP_TIME: std::time::Duration = std::time::Duration::from_secs(10);
    /// How many times the hook says "go on" before it says "stop". The
    /// hook is asked about every 100 ms while waiting, and the first
    /// back-off interval is at most 750 ms, so this lets at least one retry
    /// happen first.
    const CHECKS_BEFORE_STOP: usize = 15;

    let env = TestEnv::with_session();
    let file = env.server.mock(|when, then| {
        when.method(GET).path(DOWNLOAD_PATH);
        then.status(500).body("transient server fault");
    });
    env.mock_get_jobs(vec![ready_job_serving(
        &env.server.url(DOWNLOAD_PATH),
        1,
        &sha1_hex(b"x"),
    )]);

    let asked = std::sync::atomic::AtomicUsize::new(0);
    let stop = || asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= CHECKS_BEFORE_STOP;
    let dir = TempDir::new().expect("could not create a download directory");
    let dir_path = dir.path().display().to_string();
    let mut opts = options(&dir_path);
    opts.keep_tar = true;
    opts.retry_duration = LONG_RETRY;
    opts.should_stop = Some(&stop);

    let client = AsvoClient::new(client_config(&env)).expect("client should be created");
    let started = std::time::Instant::now();
    let err = client
        .download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
        .expect_err("the download should stop");

    assert!(matches!(err, AsvoError::Interrupted), "got {err:?}");
    assert!(file.calls() >= 2, "the transient failure was retried");
    assert!(
        started.elapsed() < MAX_STOP_TIME,
        "the stop took {:?}",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// Retries that carry on from a failed attempt
// ---------------------------------------------------------------------------
//
// A dropped connection cannot be made with the mock server, so these tests
// give the stream-untar code a reader that fails part way through, and then
// check what the next attempt does.

mod retries {
    use std::io::{self, Read};

    use httpmock::prelude::*;
    use sha1::Digest;
    use tempfile::TempDir;

    use super::{options, sha1_hex};
    use crate::asvo::apiv2::openapi::{JobDetailResponse, Type as FileType};
    use crate::asvo::download::{
        is_network_read_error, network_error, resume_point, retry_class, try_download,
        try_download_untar, untar_stream, NetworkReader, ResumePoint, RetryState, UntarCheckpoint,
    };
    use crate::asvo::{AsvoError, AsvoJob, JobFile, JobState};
    use crate::obs_id::ObsId;
    use crate::test_common::{TEST_JOB_ID, TEST_OBS_ID};
    use crate::test_config::job_type;

    /// The members of the test archive. Their sizes are not whole blocks,
    /// so each member has padding, and the middle one spans several blocks.
    const MEMBERS: [(&str, usize); 3] = [("a.dat", 1000), ("b.dat", 3000), ("c.dat", 700)];

    /// Where the data of `b.dat` starts: after the header of `a.dat` (one
    /// block), its data and padding (two blocks), and its own header.
    const B_DATA_POS: u64 = 4 * 512;

    /// Where the header of `c.dat` starts.
    const C_HEADER_POS: u64 = 10 * 512;

    /// The most bytes that the failing reader gives in one read, so that a
    /// member is written in several chunks before the failure.
    const READ_CHUNK: usize = 100;

    /// A small copy buffer, so that a failure leaves part of a member on disk.
    const TEST_BUFFER_SIZE: usize = 64;

    const LOG_PREFIX: &str = "test:";

    /// The contents of a member: ASCII digits, which differ from block to
    /// block so that a misplaced block shows.
    fn member_contents(size: usize) -> Vec<u8> {
        (0..size).map(|i| b'0' + (i % 10) as u8).collect()
    }

    /// A tar archive of [`MEMBERS`].
    fn test_archive() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, size) in MEMBERS {
            let mut header = tar::Header::new_gnu();
            header.set_size(size as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, member_contents(size).as_slice())
                .expect("could not build the test tar");
        }
        builder.into_inner().expect("could not finish the test tar")
    }

    /// Check that every member was written, whole, into `dir`.
    fn assert_members_written(dir: &TempDir) {
        for (name, size) in MEMBERS {
            let written =
                std::fs::read(dir.path().join(name)).expect("the member should be on disk");
            assert_eq!(
                written,
                member_contents(size),
                "{name} has the wrong contents"
            );
        }
    }

    /// A reader that gives `data` in small chunks, then fails with a reset
    /// connection at `fail_at`.
    struct FailingReader<'a> {
        data: &'a [u8],
        position: usize,
        fail_at: usize,
    }

    impl Read for FailingReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.fail_at {
                return Err(io::Error::from(io::ErrorKind::ConnectionReset));
            }
            let n = buf.len().min(READ_CHUNK).min(self.fail_at - self.position);
            buf[..n].copy_from_slice(&self.data[self.position..self.position + n]);
            self.position += n;
            Ok(n)
        }
    }

    /// Run a first attempt that fails at `fail_at`, the way a dropped
    /// connection does, and return the checkpoint that it leaves.
    fn failed_first_attempt(archive: &[u8], fail_at: u64, dir: &TempDir) -> UntarCheckpoint {
        let dir_path = dir.path().display().to_string();
        let mut opts = options(&dir_path);
        opts.buffer_size = TEST_BUFFER_SIZE;
        let source = NetworkReader::new(
            FailingReader {
                data: archive,
                position: 0,
                fail_at: fail_at as usize,
            },
            None,
        );
        let mut checkpoint = None;

        let err = untar_stream(
            source,
            ResumePoint::from_start(),
            dir.path(),
            LOG_PREFIX,
            &opts,
            &mut checkpoint,
        )
        .expect_err("the first attempt should fail");

        match &err {
            AsvoError::IO(e) => assert!(is_network_read_error(e), "expected a network error"),
            other => panic!("expected an IO error, got {other:?}"),
        }
        checkpoint.expect("the first attempt should leave a checkpoint")
    }

    /// The file entry for `archive`, served from `url`.
    fn file_info(url: &str, archive: &[u8], sha1: &str) -> JobFile {
        JobFile {
            type_: FileType::Acacia,
            url: Some(url.to_string()),
            path: None,
            size: archive.len() as i64,
            sha1: Some(sha1.to_string()),
            format: None,
        }
    }

    /// A ready job, for the functions that take one.
    fn ready_job() -> AsvoJob {
        crate::test_config::asvo_job(
            ObsId::validate(TEST_OBS_ID.parse().expect("the test obsid is a number"))
                .expect("the test obsid should be valid"),
            JobDetailResponse {
                id: i64::try_from(TEST_JOB_ID).expect("a test job ID fits in i64"),
                job_type: Some(job_type("visibility")),
                job_state: JobState::Completed,
                product: None,
                created: jiff::Timestamp::UNIX_EPOCH,
                started: None,
                completed: None,
                modified: None,
                error_code: None,
                error_text: None,
                user_id: 1,
                first_name: "Test".to_string(),
                last_name: "User".to_string(),
                job_params: serde_json::Map::new(),
            },
        )
    }

    #[test]
    fn a_retry_carries_on_inside_the_member_that_the_failure_stopped() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let fail_at = B_DATA_POS + 1234;

        let checkpoint = failed_first_attempt(&archive, fail_at, &dir);
        assert_eq!(checkpoint.data_pos, B_DATA_POS);
        assert_eq!(checkpoint.size, 3000);

        let resume = resume_point(&checkpoint, true, LOG_PREFIX).expect("a resume point");
        let on_disk = std::fs::metadata(dir.path().join("b.dat"))
            .expect("b.dat should be partly written")
            .len();
        assert!(on_disk > 0, "part of b.dat should be on disk");
        assert_eq!(resume.start, B_DATA_POS + on_disk);
        assert!(resume.start <= fail_at);

        let dir_path = dir.path().display().to_string();
        let mut checkpoint = Some(checkpoint);
        let start = resume.start as usize;
        let hasher = untar_stream(
            &archive[start..],
            resume,
            dir.path(),
            LOG_PREFIX,
            &options(&dir_path),
            &mut checkpoint,
        )
        .expect("the retry should succeed");

        assert_members_written(&dir);
        // The retry only fetched the tail, but the hash covers the archive.
        assert_eq!(
            crate::helpers::to_hex(&hasher.finalize()),
            sha1_hex(&archive)
        );
    }

    #[test]
    fn a_retry_after_a_failure_between_members_starts_after_the_finished_member() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        // Inside the header of c.dat: b.dat is finished, c.dat has not started.
        let fail_at = C_HEADER_POS + 100;

        let checkpoint = failed_first_attempt(&archive, fail_at, &dir);
        assert_eq!(checkpoint.data_pos, B_DATA_POS);

        let resume = resume_point(&checkpoint, true, LOG_PREFIX).expect("a resume point");
        assert_eq!(resume.start, B_DATA_POS + 3000);

        let dir_path = dir.path().display().to_string();
        let mut checkpoint = Some(checkpoint);
        let start = resume.start as usize;
        let hasher = untar_stream(
            &archive[start..],
            resume,
            dir.path(),
            LOG_PREFIX,
            &options(&dir_path),
            &mut checkpoint,
        )
        .expect("the retry should succeed");

        assert_members_written(&dir);
        assert_eq!(
            crate::helpers::to_hex(&hasher.finalize()),
            sha1_hex(&archive)
        );
    }

    #[test]
    fn a_retry_asks_the_server_for_the_rest_of_the_archive() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let mut checkpoint = Some(failed_first_attempt(&archive, B_DATA_POS + 1234, &dir));
        let start = resume_point(checkpoint.as_ref().unwrap(), false, LOG_PREFIX)
            .expect("a resume point")
            .start;

        let server = MockServer::start();
        let tail = server.mock(|when, then| {
            when.method(GET)
                .path("/archive.tar")
                .header("range", format!("bytes={start}-").as_str());
            then.status(206).body(&archive[start as usize..]);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();

        try_download_untar(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &archive, &sha1_hex(&archive)),
            crate::test_config::TEST_ASVO_JOB_ID,
            dir.path(),
            LOG_PREFIX,
            &options(&dir_path),
            &sha1_hex(&archive),
            &mut checkpoint,
        )
        .expect("the retry should succeed and match the hash");

        assert_eq!(tail.calls(), 1, "the range request should be made once");
        assert_members_written(&dir);
        assert!(
            checkpoint.is_none(),
            "a finished download keeps no checkpoint"
        );
    }

    #[test]
    fn a_retry_starts_again_when_the_server_sends_the_whole_archive() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let mut checkpoint = Some(failed_first_attempt(&archive, B_DATA_POS + 1234, &dir));

        let server = MockServer::start();
        let whole = server.mock(|when, then| {
            when.method(GET).path("/archive.tar");
            then.status(200).body(&archive);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();

        try_download_untar(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &archive, &sha1_hex(&archive)),
            crate::test_config::TEST_ASVO_JOB_ID,
            dir.path(),
            LOG_PREFIX,
            &options(&dir_path),
            &sha1_hex(&archive),
            &mut checkpoint,
        )
        .expect("the download should start again and succeed");

        assert_eq!(whole.calls(), 1);
        assert_members_written(&dir);
    }

    #[test]
    fn a_stream_untar_hash_mismatch_makes_the_retry_start_again() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/archive.tar");
            then.status(200).body(&archive);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();
        let wrong_hash = sha1_hex(b"not the archive");
        let mut checkpoint = None;

        let err = try_download_untar(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &archive, &wrong_hash),
            crate::test_config::TEST_ASVO_JOB_ID,
            dir.path(),
            LOG_PREFIX,
            &options(&dir_path),
            &wrong_hash,
            &mut checkpoint,
        )
        .expect_err("the hash should not match");

        assert!(matches!(err, AsvoError::HashMismatch { .. }), "got {err:?}");
        assert!(
            checkpoint.is_none(),
            "the retry must fetch the whole archive"
        );
    }

    /// `--skip-hash` skips the check of a download that runs from start to
    /// end in one attempt.
    #[test]
    fn without_a_hash_check_a_new_download_is_not_checked() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/archive.tar");
            then.status(200).body(&archive);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();
        let wrong_hash = sha1_hex(b"not the archive");
        let mut opts = options(&dir_path);
        opts.hash = false;
        let mut checkpoint = None;

        try_download_untar(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &archive, &wrong_hash),
            crate::test_config::TEST_ASVO_JOB_ID,
            dir.path(),
            LOG_PREFIX,
            &opts,
            &wrong_hash,
            &mut checkpoint,
        )
        .expect("a new download without a hash check should not be checked");

        assert_members_written(&dir);
    }

    /// A retry joins the bytes of two attempts, so it is checked even without
    /// a hash check. The part of `b.dat` that the first attempt wrote is
    /// changed, so only a check that reads it back can find the error.
    #[test]
    fn without_a_hash_check_a_retry_is_still_checked() {
        let archive = test_archive();
        let dir = TempDir::new().expect("could not create a download directory");
        let mut checkpoint = Some(failed_first_attempt(&archive, B_DATA_POS + 1234, &dir));
        let partial = dir.path().join("b.dat");
        let written = std::fs::read(&partial).expect("the first attempt wrote part of b.dat");
        assert!(!written.is_empty(), "the test needs part of b.dat on disk");
        std::fs::write(&partial, vec![b'x'; written.len()]).expect("could not change b.dat");
        let start = B_DATA_POS as usize + written.len();

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET)
                .path("/archive.tar")
                .header("range", format!("bytes={start}-").as_str());
            then.status(206).body(&archive[start..]);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();
        let mut opts = options(&dir_path);
        opts.hash = false;

        let err = try_download_untar(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &archive, &sha1_hex(&archive)),
            crate::test_config::TEST_ASVO_JOB_ID,
            dir.path(),
            LOG_PREFIX,
            &opts,
            &sha1_hex(&archive),
            &mut checkpoint,
        )
        .expect_err("the changed bytes should fail the hash check");

        assert!(matches!(err, AsvoError::HashMismatch { .. }), "got {err:?}");
    }

    #[test]
    fn a_keep_tar_retry_resumes_its_own_partial_file_when_no_resume_is_set() {
        let payload = member_contents(1000);
        let split = 400;
        let dir = TempDir::new().expect("could not create a download directory");
        let out_path = dir.path().join("archive.tar");
        // What the failed first attempt of this download wrote.
        std::fs::write(&out_path, &payload[..split]).expect("could not seed a partial file");

        let server = MockServer::start();
        let tail = server.mock(|when, then| {
            when.method(GET)
                .path("/archive.tar")
                .header("range", format!("bytes={split}-").as_str());
            then.status(206).body(&payload[split..]);
        });
        let url = server.url("/archive.tar");
        let dir_path = dir.path().display().to_string();
        let mut opts = options(&dir_path);
        opts.keep_tar = true;
        opts.no_resume = true;
        let mut retry_state = RetryState {
            wrote_tar: true,
            untar_checkpoint: None,
        };

        try_download(
            &reqwest::blocking::Client::new(),
            &url,
            &file_info(&url, &payload, &sha1_hex(&payload)),
            &ready_job(),
            &out_path,
            LOG_PREFIX,
            &opts,
            &mut retry_state,
        )
        .expect("the retry should resume the file and match the hash");

        assert_eq!(tail.calls(), 1, "the range request should be made once");
        assert_eq!(std::fs::read(&out_path).expect("the file"), payload);
    }

    #[test]
    fn a_download_that_ends_before_its_length_is_a_network_error() {
        let mut reader = NetworkReader::new(&b"abc"[..], Some(10));

        let err = reader
            .read_to_end(&mut Vec::new())
            .expect_err("a short download should fail");

        assert!(is_network_read_error(&err));
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn a_failed_read_of_the_download_is_retried_but_a_disk_error_is_not() {
        let network = AsvoError::IO(network_error(io::Error::from(
            io::ErrorKind::ConnectionReset,
        )));
        let disk = AsvoError::IO(io::Error::from(io::ErrorKind::PermissionDenied));

        assert!(matches!(
            retry_class(network, crate::test_config::TEST_ASVO_JOB_ID),
            backoff::Error::Transient { .. }
        ));
        assert!(matches!(
            retry_class(disk, crate::test_config::TEST_ASVO_JOB_ID),
            backoff::Error::Permanent(_)
        ));
    }

    #[test]
    fn a_network_error_shows_as_the_error_that_it_wraps() {
        let inner = io::Error::new(io::ErrorKind::ConnectionReset, "connection reset by peer");
        let shown = inner.to_string();

        assert_eq!(network_error(inner).to_string(), shown);
    }
}

// ---------------------------------------------------------------------------
// A new run that carries on from the files of an earlier run
// ---------------------------------------------------------------------------

mod reruns {
    use std::time::Duration;

    use httpmock::prelude::*;
    use tempfile::TempDir;

    use super::{options, ready_job_serving, sha1_hex, DOWNLOAD_PATH};
    use crate::asvo::download::EARLIER_FILES_WINDOW;
    use crate::asvo::{AsvoClient, AsvoError};
    use crate::test_common::*;
    use crate::test_config::client_config;

    /// The members of the test archive. `b.dat` is larger than one range of
    /// the check, so that the check needs more than one request.
    const MEMBERS: [(&str, usize); 3] = [("a.dat", 1000), ("b.dat", 150_000), ("c.dat", 700)];

    /// How much of `b.dat` a partly written file holds.
    const PARTIAL_B: u64 = 40_000;

    /// Long enough for one retry after a hash mismatch.
    const ONE_RETRY: Duration = Duration::from_secs(30);

    fn member_contents(size: usize) -> Vec<u8> {
        (0..size).map(|i| b'0' + (i % 10) as u8).collect()
    }

    fn test_archive() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, size) in MEMBERS {
            let mut header = tar::Header::new_gnu();
            header.set_size(size as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, member_contents(size).as_slice())
                .expect("could not build the test tar");
        }
        builder.into_inner().expect("could not finish the test tar")
    }

    /// The archive offset of each member's first data byte, in order.
    fn data_positions(archive: &[u8]) -> Vec<u64> {
        let mut tar = tar::Archive::new(archive);
        tar.entries()
            .expect("the test tar should be readable")
            .map(|entry| entry.expect("a test tar entry").raw_file_position())
            .collect()
    }

    /// The `Range` value of a closed range request of the check.
    fn window(start: u64, archive: &[u8]) -> String {
        let end = (start + EARLIER_FILES_WINDOW).min(archive.len() as u64);
        format!("bytes={start}-{}", end - 1)
    }

    /// Write members into `dir` as an earlier run would have.
    fn write_members(dir: &TempDir, names: &[&str]) {
        for (name, size) in MEMBERS {
            if names.contains(&name) {
                std::fs::write(dir.path().join(name), member_contents(size))
                    .expect("could not write a member");
            }
        }
    }

    fn assert_members_written(dir: &TempDir) {
        for (name, size) in MEMBERS {
            let written =
                std::fs::read(dir.path().join(name)).expect("the member should be on disk");
            assert_eq!(
                written,
                member_contents(size),
                "{name} has the wrong contents"
            );
        }
    }

    /// A test environment whose ready job serves `archive`.
    fn env_serving(archive: &[u8]) -> TestEnv {
        let env = TestEnv::with_session();
        env.mock_get_jobs(vec![ready_job_serving(
            &env.server.url(DOWNLOAD_PATH),
            archive.len() as u64,
            &sha1_hex(archive),
        )]);
        env
    }

    /// Mock one closed range request of the check.
    fn mock_window<'a>(env: &'a TestEnv, start: u64, archive: &[u8]) -> httpmock::Mock<'a> {
        let end = (start + EARLIER_FILES_WINDOW).min(archive.len() as u64);
        env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", window(start, archive).as_str());
            then.status(206)
                .body(&archive[start as usize..end as usize]);
        })
    }

    /// Mock the open-ended range request of the download itself.
    fn mock_rest<'a>(env: &'a TestEnv, start: u64, archive: &[u8]) -> httpmock::Mock<'a> {
        env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", format!("bytes={start}-").as_str());
            then.status(206).body(&archive[start as usize..]);
        })
    }

    /// Mock a request for the whole archive.
    fn mock_whole<'a>(env: &'a TestEnv, archive: &[u8]) -> httpmock::Mock<'a> {
        env.server.mock(|when, then| {
            when.method(GET).path(DOWNLOAD_PATH).header_missing("range");
            then.status(200).body(archive);
        })
    }

    fn download(
        env: &TestEnv,
        dir: &TempDir,
        configure: impl FnOnce(&mut crate::asvo::DownloadOptions),
    ) -> Result<(), AsvoError> {
        let dir_path = dir.path().display().to_string();
        let mut opts = options(&dir_path);
        configure(&mut opts);
        let client = AsvoClient::new(client_config(env)).expect("client should be created");
        client.download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
    }

    #[test]
    fn a_rerun_fetches_only_the_members_that_are_not_on_disk() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let b_end = data[1] + 150_000;
        let env = env_serving(&archive);
        let first = mock_window(&env, 0, &archive);
        let after_b = mock_window(&env, b_end, &archive);
        let rest = mock_rest(&env, data[2], &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat", "b.dat"]);

        download(&env, &dir, |_| {}).expect("the rerun should succeed and match the hash");

        assert_eq!(first.calls(), 1);
        assert_eq!(after_b.calls(), 1, "b.dat's data should come from disk");
        assert_eq!(rest.calls(), 1, "only c.dat should be fetched");
        assert_members_written(&dir);
    }

    #[test]
    fn a_rerun_carries_on_inside_a_partly_written_file() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let env = env_serving(&archive);
        let first = mock_window(&env, 0, &archive);
        let rest = mock_rest(&env, data[1] + PARTIAL_B, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat"]);
        std::fs::write(
            dir.path().join("b.dat"),
            &member_contents(150_000)[..PARTIAL_B as usize],
        )
        .expect("could not write a partial member");

        download(&env, &dir, |_| {}).expect("the rerun should succeed and match the hash");

        assert_eq!(first.calls(), 1);
        assert_eq!(rest.calls(), 1);
        assert_members_written(&dir);
    }

    #[test]
    fn a_rerun_with_every_member_on_disk_fetches_only_the_end_of_the_archive() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let env = env_serving(&archive);
        let first = mock_window(&env, 0, &archive);
        let after_b = mock_window(&env, data[1] + 150_000, &archive);
        let end = mock_rest(&env, data[2] + 700, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat", "b.dat", "c.dat"]);

        download(&env, &dir, |_| {}).expect("the rerun should succeed and match the hash");

        assert_eq!(first.calls(), 1);
        assert_eq!(after_b.calls(), 1);
        assert_eq!(
            end.calls(),
            1,
            "only the padding and end blocks should be fetched"
        );
        assert_members_written(&dir);
    }

    #[test]
    fn a_rerun_with_a_wrong_file_on_disk_fetches_the_whole_archive_again() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let env = env_serving(&archive);
        mock_window(&env, 0, &archive);
        let rest = mock_rest(&env, data[1], &archive);
        let whole = mock_whole(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        // The right size, the wrong contents.
        std::fs::write(dir.path().join("a.dat"), vec![b'x'; 1000]).expect("could not write a.dat");

        download(&env, &dir, |opts| opts.retry_duration = ONE_RETRY)
            .expect("the retry should fetch the whole archive and succeed");

        assert_eq!(rest.calls(), 1, "the first attempt used the file on disk");
        assert_eq!(whole.calls(), 1, "the retry fetched the whole archive");
        assert_members_written(&dir);
    }

    #[test]
    fn a_rerun_with_no_resume_set_fetches_the_whole_archive() {
        let archive = test_archive();
        let env = env_serving(&archive);
        let first = mock_window(&env, 0, &archive);
        let whole = mock_whole(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat", "b.dat"]);

        download(&env, &dir, |opts| opts.no_resume = true).expect("the download should succeed");

        assert_eq!(
            first.calls(),
            0,
            "no files from an earlier run are looked for"
        );
        assert_eq!(whole.calls(), 1);
        assert_members_written(&dir);
    }

    #[test]
    fn a_directory_without_files_of_the_archive_costs_one_small_request() {
        let archive = test_archive();
        let env = env_serving(&archive);
        let first = mock_window(&env, 0, &archive);
        let whole = mock_whole(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        std::fs::write(dir.path().join("unrelated.txt"), "not from the archive")
            .expect("could not write an unrelated file");

        download(&env, &dir, |_| {}).expect("the download should succeed");

        assert_eq!(first.calls(), 1);
        assert_eq!(whole.calls(), 1);
        assert_members_written(&dir);
    }

    #[test]
    fn a_server_that_ignores_range_requests_gets_a_whole_download() {
        let archive = test_archive();
        let env = env_serving(&archive);
        // Answers every request, with or without a range, with the whole archive.
        let any = env.server.mock(|when, then| {
            when.method(GET).path(DOWNLOAD_PATH);
            then.status(200).body(&archive);
        });
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat"]);

        download(&env, &dir, |_| {}).expect("the download should succeed");

        assert_eq!(any.calls(), 2, "the check, then the whole download");
        assert_members_written(&dir);
    }

    #[test]
    fn without_a_hash_check_a_reused_file_is_still_checked() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let env = env_serving(&archive);
        mock_window(&env, 0, &archive);
        let rest = mock_rest(&env, data[1], &archive);
        let whole = mock_whole(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        // The right size, the wrong contents.
        std::fs::write(dir.path().join("a.dat"), vec![b'x'; 1000]).expect("could not write a.dat");

        download(&env, &dir, |opts| {
            opts.hash = false;
            opts.retry_duration = ONE_RETRY;
        })
        .expect("the retry should fetch the whole archive and succeed");

        assert_eq!(rest.calls(), 1, "the first attempt used the file on disk");
        assert_eq!(
            whole.calls(),
            1,
            "the hash failed, so the retry fetched the whole archive"
        );
        assert_members_written(&dir);
    }

    #[test]
    fn without_a_hash_check_correct_files_on_disk_are_used() {
        let archive = test_archive();
        let data = data_positions(&archive);
        let env = env_serving(&archive);
        mock_window(&env, 0, &archive);
        let rest = mock_rest(&env, data[1], &archive);
        let whole = mock_whole(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        write_members(&dir, &["a.dat"]);

        download(&env, &dir, |opts| opts.hash = false).expect("the rerun should succeed");

        assert_eq!(rest.calls(), 1);
        assert_eq!(whole.calls(), 0);
        assert_members_written(&dir);
    }
}

// ---------------------------------------------------------------------------
// Tar entries whose path is not inside the download directory
// ---------------------------------------------------------------------------

mod unsafe_paths {
    use httpmock::prelude::*;
    use sha1::{Digest, Sha1};
    use tempfile::TempDir;

    use super::{options, ready_job_serving, sha1_hex, DOWNLOAD_PATH};
    use crate::asvo::download::{untar_stream, ResumePoint, EARLIER_FILES_WINDOW};
    use crate::asvo::AsvoClient;
    use crate::test_common::*;
    use crate::test_config::client_config;

    const LOG_PREFIX: &str = "test:";

    /// A tar archive of `members`, with each name written into the header
    /// as it is. `tar::Builder` refuses unsafe names, so the header is set
    /// by hand.
    fn archive_with(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in members {
            let mut header = tar::Header::new_gnu();
            let gnu = header.as_gnu_mut().expect("a GNU header");
            gnu.name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append(&header, *data)
                .expect("could not build the test tar");
        }
        builder.into_inner().expect("could not finish the test tar")
    }

    /// Unpack `archive` into `unpack_dir` and check that the hash still
    /// covers the whole archive.
    fn unpack(archive: &[u8], unpack_dir: &std::path::Path) {
        let dir_path = unpack_dir.display().to_string();
        let mut checkpoint = None;
        let hasher = untar_stream(
            archive,
            ResumePoint::from_start(),
            unpack_dir,
            LOG_PREFIX,
            &options(&dir_path),
            &mut checkpoint,
        )
        .expect("the archive should be unpacked");
        assert_eq!(hasher.finalize()[..], Sha1::digest(archive)[..]);
    }

    #[test]
    fn an_entry_with_a_parent_dir_path_is_skipped() {
        let dir = TempDir::new().expect("could not create a directory");
        let unpack_dir = dir.path().join("download");
        std::fs::create_dir(&unpack_dir).expect("could not create the download directory");
        let archive = archive_with(&[("../escaped.dat", b"outside"), ("good.dat", b"inside")]);

        unpack(&archive, &unpack_dir);

        assert!(
            !dir.path().join("escaped.dat").exists(),
            "nothing may be written outside"
        );
        assert_eq!(
            std::fs::read(unpack_dir.join("good.dat")).expect("good.dat"),
            b"inside"
        );
    }

    #[test]
    fn an_entry_with_an_absolute_path_is_skipped() {
        let dir = TempDir::new().expect("could not create a download directory");
        let elsewhere = TempDir::new().expect("could not create a second directory");
        let absolute = elsewhere.path().join("escaped.dat").display().to_string();
        let archive = archive_with(&[(absolute.as_str(), b"outside"), ("good.dat", b"inside")]);

        unpack(&archive, dir.path());

        assert!(
            !elsewhere.path().join("escaped.dat").exists(),
            "nothing may be written outside"
        );
        assert_eq!(
            std::fs::read(dir.path().join("good.dat")).expect("good.dat"),
            b"inside"
        );
    }

    /// A file entry whose path names no file (empty, or only `.`) would be
    /// written to the download directory itself. It is skipped, and the
    /// entries after it are still unpacked.
    #[test]
    fn an_entry_with_no_name_is_skipped() {
        for name in ["", "."] {
            let dir = TempDir::new().expect("could not create a download directory");
            let archive = archive_with(&[(name, b"no name"), ("good.dat", b"inside")]);

            unpack(&archive, dir.path());

            let written: Vec<_> = std::fs::read_dir(dir.path())
                .expect("could not read the download directory")
                .map(|entry| entry.expect("a directory entry").file_name())
                .collect();
            assert_eq!(written, ["good.dat"], "entry name {name:?}");
            assert_eq!(
                std::fs::read(dir.path().join("good.dat")).expect("good.dat"),
                b"inside"
            );
        }
    }

    /// An entry whose file is already a symbolic link (here to a file
    /// outside the download directory) is not written through the link.
    #[cfg(unix)]
    #[test]
    fn an_entry_whose_file_is_a_symlink_is_skipped() {
        let dir = TempDir::new().expect("could not create a download directory");
        let elsewhere = TempDir::new().expect("could not create a second directory");
        let target = elsewhere.path().join("victim.dat");
        std::fs::write(&target, b"untouched").expect("could not write the target");
        std::os::unix::fs::symlink(&target, dir.path().join("link.dat"))
            .expect("could not make the link");
        let archive = archive_with(&[("link.dat", b"outside"), ("good.dat", b"inside")]);

        unpack(&archive, dir.path());

        assert_eq!(std::fs::read(&target).expect("the target"), b"untouched");
        assert_eq!(
            std::fs::read(dir.path().join("good.dat")).expect("good.dat"),
            b"inside"
        );
    }

    /// An entry below a directory that is a symbolic link is not written
    /// through the link.
    #[cfg(unix)]
    #[test]
    fn an_entry_below_a_symlinked_directory_is_skipped() {
        let dir = TempDir::new().expect("could not create a download directory");
        let elsewhere = TempDir::new().expect("could not create a second directory");
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("sub"))
            .expect("could not make the link");
        let archive = archive_with(&[("sub/escaped.dat", b"outside"), ("good.dat", b"inside")]);

        unpack(&archive, dir.path());

        assert!(
            !elsewhere.path().join("escaped.dat").exists(),
            "nothing may be written through the link"
        );
        assert_eq!(
            std::fs::read(dir.path().join("good.dat")).expect("good.dat"),
            b"inside"
        );
    }

    #[test]
    fn a_rerun_carries_on_from_a_skipped_entry() {
        let a_data = vec![b'a'; 1000];
        let archive = archive_with(&[
            ("a.dat", &a_data),
            ("../escaped.dat", &[b'e'; 600]),
            ("c.dat", &[b'c'; 700]),
        ]);
        let escaped_data_pos = {
            let mut tar = tar::Archive::new(archive.as_slice());
            let mut entries = tar.entries().expect("the test tar should be readable");
            entries.next();
            entries
                .next()
                .expect("a second entry")
                .expect("a readable entry")
                .raw_file_position()
        };

        let env = TestEnv::with_session();
        env.mock_get_jobs(vec![ready_job_serving(
            &env.server.url(DOWNLOAD_PATH),
            archive.len() as u64,
            &sha1_hex(&archive),
        )]);
        let window_end = EARLIER_FILES_WINDOW.min(archive.len() as u64);
        let check = env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", format!("bytes=0-{}", window_end - 1).as_str());
            then.status(206).body(&archive[..window_end as usize]);
        });
        let rest = env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", format!("bytes={escaped_data_pos}-").as_str());
            then.status(206).body(&archive[escaped_data_pos as usize..]);
        });

        let dir = TempDir::new().expect("could not create a directory");
        let unpack_dir = dir.path().join("download");
        std::fs::create_dir(&unpack_dir).expect("could not create the download directory");
        std::fs::write(unpack_dir.join("a.dat"), &a_data).expect("could not write a.dat");
        let dir_path = unpack_dir.display().to_string();

        let client = AsvoClient::new(client_config(&env)).expect("client should be created");
        client
            .download_job(crate::test_config::TEST_ASVO_JOB_ID, &options(&dir_path))
            .expect("the rerun should succeed and match the hash");

        assert_eq!(check.calls(), 1);
        assert_eq!(
            rest.calls(),
            1,
            "the download carries on from the skipped entry"
        );
        assert!(
            !dir.path().join("escaped.dat").exists(),
            "nothing may be written outside"
        );
        assert_eq!(
            std::fs::read(unpack_dir.join("c.dat")).expect("c.dat"),
            vec![b'c'; 700]
        );
    }
}

// ---------------------------------------------------------------------------
// The resume file (sidecar) of a stream-untar download
// ---------------------------------------------------------------------------

mod sidecar {
    use std::time::{Duration, SystemTime};

    use httpmock::prelude::*;
    use serde_json::Value;
    use tempfile::TempDir;

    use super::{options, ready_job_serving, sha1_hex, DOWNLOAD_FILE, DOWNLOAD_PATH};
    use crate::asvo::download::{EARLIER_FILES_WINDOW, SIDECAR_SUFFIX};
    use crate::asvo::{AsvoClient, AsvoError, DownloadOptions};
    use crate::test_common::*;
    use crate::test_config::client_config;

    const MEMBERS: [(&str, usize); 3] = [("a.dat", 1000), ("b.dat", 3000), ("c.dat", 700)];

    fn member_contents(size: usize) -> Vec<u8> {
        (0..size).map(|i| b'0' + (i % 10) as u8).collect()
    }

    fn test_archive() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, size) in MEMBERS {
            let mut header = tar::Header::new_gnu();
            header.set_size(size as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, member_contents(size).as_slice())
                .expect("could not build the test tar");
        }
        builder.into_inner().expect("could not finish the test tar")
    }

    fn c_data_pos(archive: &[u8]) -> u64 {
        let mut tar = tar::Archive::new(archive);
        tar.entries()
            .expect("the test tar should be readable")
            .nth(2)
            .expect("a third entry")
            .expect("a readable entry")
            .raw_file_position()
    }

    fn sidecar_file(dir: &TempDir) -> std::path::PathBuf {
        dir.path().join(format!(".{DOWNLOAD_FILE}{SIDECAR_SUFFIX}"))
    }

    fn assert_members_written(dir: &TempDir) {
        for (name, size) in MEMBERS {
            let written =
                std::fs::read(dir.path().join(name)).expect("the member should be on disk");
            assert_eq!(
                written,
                member_contents(size),
                "{name} has the wrong contents"
            );
        }
    }

    /// The mock server and the requests of the tests: the check's range
    /// request, the whole archive, and the rest of the archive from `c.dat`.
    struct Server<'a> {
        env: &'a TestEnv,
        check: httpmock::Mock<'a>,
        rest: httpmock::Mock<'a>,
    }

    fn serve<'a>(env: &'a TestEnv, archive: &[u8]) -> Server<'a> {
        env.mock_get_jobs(vec![ready_job_serving(
            &env.server.url(DOWNLOAD_PATH),
            archive.len() as u64,
            &sha1_hex(archive),
        )]);
        let window_end = EARLIER_FILES_WINDOW.min(archive.len() as u64);
        let check = env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", format!("bytes=0-{}", window_end - 1).as_str());
            then.status(206).body(&archive[..window_end as usize]);
        });
        env.server.mock(|when, then| {
            when.method(GET).path(DOWNLOAD_PATH).header_missing("range");
            then.status(200).body(archive);
        });
        let start = c_data_pos(archive);
        let rest = env.server.mock(|when, then| {
            when.method(GET)
                .path(DOWNLOAD_PATH)
                .header("range", format!("bytes={start}-").as_str());
            then.status(206).body(&archive[start as usize..]);
        });
        Server { env, check, rest }
    }

    fn download(
        server: &Server,
        dir: &TempDir,
        configure: impl FnOnce(&mut DownloadOptions),
    ) -> Result<(), AsvoError> {
        let dir_path = dir.path().display().to_string();
        let mut opts = options(&dir_path);
        configure(&mut opts);
        let client = AsvoClient::new(client_config(server.env)).expect("client should be created");
        client.download_job(crate::test_config::TEST_ASVO_JOB_ID, &opts)
    }

    /// A first run that writes `a.dat` and `b.dat` and then fails: a
    /// directory is in the place of `c.dat`. The directory is then removed,
    /// as a user would fix the problem.
    fn failed_first_run(server: &Server, dir: &TempDir) {
        std::fs::create_dir(dir.path().join("c.dat")).expect("could not create the obstacle");
        let err = download(server, dir, |_| {}).expect_err("writing c.dat should fail");
        assert!(matches!(err, AsvoError::IO(_)), "got {err:?}");
        std::fs::remove_dir(dir.path().join("c.dat")).expect("could not remove the obstacle");
        assert!(
            sidecar_file(dir).exists(),
            "the failed run should leave a resume file"
        );
    }

    /// Change one field of the resume file.
    fn edit_sidecar(dir: &TempDir, edit: impl FnOnce(&mut Value)) {
        let path = sidecar_file(dir);
        let mut sidecar: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("the resume file"))
                .expect("the resume file should be JSON");
        edit(&mut sidecar);
        std::fs::write(&path, serde_json::to_vec(&sidecar).expect("JSON"))
            .expect("could not write the resume file");
    }

    #[test]
    fn a_rerun_uses_the_resume_file_of_a_failed_run() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);
        let checks_before = server.check.calls();

        download(&server, &dir, |_| {}).expect("the rerun should succeed and match the hash");

        assert_eq!(
            server.check.calls(),
            checks_before,
            "the resume file replaces the check"
        );
        assert_eq!(server.rest.calls(), 1, "only c.dat should be fetched");
        assert_members_written(&dir);
        assert!(
            !sidecar_file(&dir).exists(),
            "a finished download deletes its resume file"
        );
    }

    #[test]
    fn a_rerun_without_a_hash_check_still_checks_the_hash_from_the_resume_file() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);
        // The saved hash state is wrong, so only the hash check can notice.
        edit_sidecar(&dir, |sidecar| {
            let state = sidecar["hasher_state"]
                .as_str()
                .expect("a state")
                .to_string();
            let mut bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, state)
                    .expect("base64");
            bytes[0] ^= 0xff;
            sidecar["hasher_state"] =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes).into();
        });

        let err = download(&server, &dir, |opts| opts.hash = false)
            .expect_err("the forced hash check should fail");

        assert!(matches!(err, AsvoError::HashMismatch { .. }), "got {err:?}");
        assert!(
            !sidecar_file(&dir).exists(),
            "a failed hash deletes the resume file"
        );
    }

    #[test]
    fn a_resume_file_is_not_used_when_a_file_changed_after_it() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);
        let checks_before = server.check.calls();
        // Same contents, a new modification time.
        let a = std::fs::File::options()
            .write(true)
            .open(dir.path().join("a.dat"))
            .expect("a.dat");
        a.set_modified(SystemTime::now() + Duration::from_secs(60))
            .expect("could not set the modification time");
        drop(a);

        download(&server, &dir, |_| {}).expect("the rerun should succeed");

        assert_eq!(
            server.check.calls(),
            checks_before + 1,
            "the files on disk are checked instead"
        );
        assert_eq!(server.rest.calls(), 1);
        assert_members_written(&dir);
    }

    #[test]
    fn a_resume_file_for_another_archive_is_not_used() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);
        let checks_before = server.check.calls();
        edit_sidecar(&dir, |sidecar| {
            sidecar["archive_sha1"] = sha1_hex(b"another archive").into()
        });

        download(&server, &dir, |_| {}).expect("the rerun should succeed");

        assert_eq!(
            server.check.calls(),
            checks_before + 1,
            "the files on disk are checked instead"
        );
        assert_members_written(&dir);
    }

    #[test]
    fn a_resume_file_with_an_unsafe_path_is_not_used() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);
        let checks_before = server.check.calls();
        edit_sidecar(&dir, |sidecar| {
            sidecar["files"][0]["path"] = "../a.dat".into()
        });

        download(&server, &dir, |_| {}).expect("the rerun should succeed");

        assert_eq!(
            server.check.calls(),
            checks_before + 1,
            "the files on disk are checked instead"
        );
        assert_members_written(&dir);
    }

    #[test]
    fn no_resume_ignores_the_resume_file_and_deletes_it_when_finished() {
        let archive = test_archive();
        let env = TestEnv::with_session();
        let server = serve(&env, &archive);
        let dir = TempDir::new().expect("could not create a download directory");
        failed_first_run(&server, &dir);

        download(&server, &dir, |opts| opts.no_resume = true).expect("the download should succeed");

        assert_eq!(server.rest.calls(), 0, "the whole archive is fetched");
        assert_members_written(&dir);
        assert!(!sidecar_file(&dir).exists());
    }
}
