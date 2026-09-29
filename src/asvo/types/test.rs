// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super`] ASVO data types.

use super::*;

/// An obsid used by these tests.
const OBSID: u64 = 1065880128;
/// Job IDs used by these tests.
const JOBID_A: AsvoJobID = 101;
const JOBID_B: AsvoJobID = 102;

/// A job with the given ID and state.
fn job(jobid: AsvoJobID, state: AsvoJobState) -> AsvoJob {
    AsvoJob {
        obsid: Obsid::validate(OBSID).expect("the test obsid should be valid"),
        jobid,
        jtype: AsvoJobType::DownloadVisibilities,
        state,
        files: None,
        completed: None,
    }
}

#[test]
fn all_ready_is_true_when_every_job_is_ready() {
    let jobs = AsvoJobVec(vec![
        job(JOBID_A, AsvoJobState::Ready),
        job(JOBID_B, AsvoJobState::Ready),
    ]);
    assert!(jobs
        .all_ready(&[JOBID_A, JOBID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_is_false_while_a_job_is_in_progress() {
    let jobs = AsvoJobVec(vec![
        job(JOBID_A, AsvoJobState::Ready),
        job(JOBID_B, AsvoJobState::Queued),
    ]);
    assert!(!jobs
        .all_ready(&[JOBID_A, JOBID_B])
        .expect("no job has failed"));
}

#[test]
fn all_ready_ignores_jobs_that_were_not_asked_about() {
    let jobs = AsvoJobVec(vec![
        job(JOBID_A, AsvoJobState::Ready),
        job(JOBID_B, AsvoJobState::Cancelled),
    ]);
    assert!(jobs.all_ready(&[JOBID_A]).expect("no job has failed"));
}

#[test]
fn all_ready_reports_an_unknown_job() {
    let jobs = AsvoJobVec(vec![job(JOBID_A, AsvoJobState::Ready)]);
    let err = jobs.all_ready(&[JOBID_B]).expect_err("expected an error");
    assert!(
        matches!(err, AsvoError::NoAsvoJob(jobid) if jobid == JOBID_B),
        "got {err:?}"
    );
}

#[test]
fn all_ready_reports_a_failed_job_with_its_error() {
    let jobs = AsvoJobVec(vec![job(
        JOBID_A,
        AsvoJobState::Error("the conversion failed".to_string()),
    )]);
    match jobs.all_ready(&[JOBID_A]) {
        Err(AsvoError::JobFailed {
            jobid,
            obsid,
            error,
        }) => {
            assert_eq!(jobid, JOBID_A);
            assert_eq!(u64::from(obsid), OBSID);
            assert_eq!(error, "the conversion failed");
        }
        other => panic!("expected JobFailed, got {other:?}"),
    }
}

#[test]
fn all_ready_reports_an_expired_or_cancelled_job() {
    let expired = AsvoJobVec(vec![job(JOBID_A, AsvoJobState::Expired)]);
    assert!(matches!(
        expired.all_ready(&[JOBID_A]),
        Err(AsvoError::JobExpired(JOBID_A))
    ));

    let cancelled = AsvoJobVec(vec![job(JOBID_A, AsvoJobState::Cancelled)]);
    assert!(matches!(
        cancelled.all_ready(&[JOBID_A]),
        Err(AsvoError::JobCancelled(JOBID_A))
    ));
}

#[test]
fn all_ready_reports_the_first_failure_in_the_order_asked() {
    let jobs = AsvoJobVec(vec![
        job(JOBID_A, AsvoJobState::Expired),
        job(JOBID_B, AsvoJobState::Cancelled),
    ]);
    assert!(matches!(
        jobs.all_ready(&[JOBID_B, JOBID_A]),
        Err(AsvoError::JobCancelled(JOBID_B))
    ));
}
