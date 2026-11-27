// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The wait for jobs to be ready: [`MwaAsvoClient::wait_for_jobs`].

use std::collections::BTreeMap;
use std::time::Duration;

use log::info;

use super::api::client::{JobsFilter, MwaAsvoClient};
use super::api::openapi::JobState;
use super::retry::sleep_unless_stopped;
use super::{MwaAsvoError, MwaAsvoJobId, MwaAsvoJobVec};

#[cfg(test)]
mod tests;

/// The default time before the first job list request of a wait, so that
/// the user's queue is hopefully current.
pub const DEFAULT_WAIT_INITIAL_DELAY: Duration = Duration::from_secs(1);

/// The default time between two job list requests of a wait.
pub const DEFAULT_WAIT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// The default time that a wait tries a failing job list request again (a
/// network error, an HTTP 5xx or 429 reply), from the first failure. A
/// request that succeeds starts the time again.
pub const DEFAULT_WAIT_RETRY_DURATION: Duration = Duration::from_secs(900);

/// How [`MwaAsvoClient::wait_for_jobs`] waits. [`Default`] gives the
/// `DEFAULT_WAIT_*` times, and no `should_stop`.
pub struct WaitOptions<'a> {
    /// The time before the first job list request.
    pub initial_delay: Duration,
    /// The time between two job list requests.
    pub poll_interval: Duration,
    /// How long a failing job list request is tried again (see
    /// [`DEFAULT_WAIT_RETRY_DURATION`]). Zero does not try again.
    pub retry_duration: Duration,
    /// Asked during each pause. When it returns `true`, the wait ends with
    /// [`MwaAsvoError::Interrupted`]. `None` never stops.
    pub should_stop: Option<&'a dyn Fn() -> bool>,
}

impl Default for WaitOptions<'_> {
    fn default() -> Self {
        Self {
            initial_delay: DEFAULT_WAIT_INITIAL_DELAY,
            poll_interval: DEFAULT_WAIT_POLL_INTERVAL,
            retry_duration: DEFAULT_WAIT_RETRY_DURATION,
            should_stop: None,
        }
    }
}

impl MwaAsvoClient {
    /// Get the job list until all of `job_ids` are ready (see
    /// [`MwaAsvoJobVec::all_ready`]), as `giant-squid wait` does, and
    /// return the last job list. The state of each job is logged (at the
    /// `info` level) the first time and when it changes.
    ///
    /// The jobs are looked for in the job list of the API's default window
    /// (30 days by default), as [`Self::get_jobs`] with no filter has them.
    ///
    /// # Errors
    ///
    /// The error of [`MwaAsvoJobVec::all_ready`] as soon as a job is missing,
    /// has an error, has been cancelled or has expired download links;
    /// [`MwaAsvoError::Api`] when the job list request fails (after the
    /// retries of [`WaitOptions::retry_duration`]); and
    /// [`MwaAsvoError::Interrupted`] when `should_stop` says so.
    pub fn wait_for_jobs(
        &self,
        job_ids: &[MwaAsvoJobId],
        opts: &WaitOptions,
    ) -> Result<MwaAsvoJobVec, MwaAsvoError> {
        let job_count = job_ids.len();
        info!("Waiting for {job_count} jobs to be ready...");
        let mut last_state = BTreeMap::<MwaAsvoJobId, JobState>::new();
        sleep_unless_stopped(opts.initial_delay, opts.should_stop)?;
        loop {
            let jobs = self.get_jobs_retrying(
                &JobsFilter::default(),
                opts.retry_duration,
                opts.should_stop,
            )?;
            let all_ready = jobs.all_ready(job_ids)?;

            // `all_ready` has already checked that every job is in the list.
            for job in job_ids
                .iter()
                .filter_map(|id| jobs.0.iter().find(|j| j.job_id() == *id))
            {
                if last_state.insert(job.job_id(), job.job_state) != Some(job.job_state) {
                    let job_id = job.job_id();
                    let obs_id = job.obs_id();
                    let state = job.display_state();
                    info!("Job ID {job_id} (Obs ID: {obs_id}): is {state}");
                }
            }

            if all_ready {
                info!("All {job_count} MWA ASVO jobs are ready for download.");
                return Ok(jobs);
            }
            sleep_unless_stopped(opts.poll_interval, opts.should_stop)?;
        }
    }
}
