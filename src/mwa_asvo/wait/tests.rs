//! The wait for jobs: [`MwaAsvoClient::wait_for_jobs`].

use std::time::Duration;

use super::WaitOptions;
use crate::mwa_asvo::{MwaAsvoClient, MwaAsvoError};
use crate::test_common::*;
use crate::test_config::{client_config, job_id};

/// No pause and no retry, so that a test does not sleep.
fn no_pause() -> WaitOptions<'static> {
    WaitOptions {
        initial_delay: Duration::ZERO,
        poll_interval: Duration::ZERO,
        retry_duration: Duration::ZERO,
        should_stop: None,
    }
}

#[test]
fn a_wait_for_ready_jobs_returns_the_job_list() {
    let env = TestEnv::with_session();
    let list = env.mock_get_jobs(vec![
        job_detail(1, TEST_OBS_ID, "completed", 1),
        job_detail(2, TEST_OBS_ID, "completed", 1),
    ]);
    let client = MwaAsvoClient::new(client_config(&env)).expect("client should be created");

    let jobs = client
        .wait_for_jobs(&[job_id(1), job_id(2)], &no_pause())
        .expect("the jobs are ready");

    assert_eq!(jobs.0.len(), 2);
    assert_eq!(list.calls(), 1);
}

#[test]
fn a_wait_fails_as_soon_as_a_job_has_failed() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(1, TEST_OBS_ID, "error", 1)]);
    let client = MwaAsvoClient::new(client_config(&env)).expect("client should be created");

    let err = client
        .wait_for_jobs(&[job_id(1)], &no_pause())
        .expect_err("the job has failed");

    assert!(matches!(err, MwaAsvoError::JobFailed { .. }), "{err:?}");
}

/// `should_stop` ends the wait at the next pause.
#[test]
fn a_wait_ends_when_the_caller_asks() {
    let env = TestEnv::with_session();
    env.mock_get_jobs(vec![job_detail(1, TEST_OBS_ID, "queued", 1)]);
    let client = MwaAsvoClient::new(client_config(&env)).expect("client should be created");
    let opts = WaitOptions {
        poll_interval: Duration::from_secs(60),
        should_stop: Some(&|| true),
        ..no_pause()
    };

    let err = client
        .wait_for_jobs(&[job_id(1)], &opts)
        .expect_err("the caller asked to stop");

    assert!(matches!(err, MwaAsvoError::Interrupted), "{err:?}");
}
