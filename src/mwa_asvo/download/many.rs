// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The download of several jobs at a time: [`MwaAsvoClient::download_many`].

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use log::info;

use super::{ready_job_of_obs_id, DownloadOptions, DownloadProgress};
use crate::mwa_asvo::{
    JobsFilter, MwaAsvoClient, MwaAsvoError, MwaAsvoJob, MwaAsvoJobId, MwaAsvoJobVec,
};
use crate::obs_id::ObsId;

/// The default number of downloads that [`MwaAsvoClient::download_many`] runs
/// at the same time, as `giant-squid download` does.
pub const DEFAULT_CONCURRENT_DOWNLOADS: NonZeroUsize = NonZeroUsize::new(4).unwrap();

/// The time between the starts of two downloads, so that the downloads start
/// (and log) in their order: 1/20 before 2/20. It is for the display only.
const DOWNLOAD_START_DELAY: Duration = Duration::from_millis(50);

/// What one download of [`MwaAsvoClient::download_many`] is for: a Job ID,
/// or the ready job of an Obs ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadTarget {
    /// The job with this Job ID.
    Job(MwaAsvoJobId),
    /// The one ready job of this Obs ID.
    Obs(ObsId),
}

impl DownloadTarget {
    /// The targets of `job_ids` and then of `obs_ids`, in that order.
    pub fn all(job_ids: &[MwaAsvoJobId], obs_ids: &[ObsId]) -> Vec<Self> {
        job_ids
            .iter()
            .map(|j| Self::Job(*j))
            .chain(obs_ids.iter().map(|o| Self::Obs(*o)))
            .collect()
    }

    /// The Job ID that was asked for, if it is one.
    pub fn job_id(self) -> Option<MwaAsvoJobId> {
        match self {
            Self::Job(job_id) => Some(job_id),
            Self::Obs(_) => None,
        }
    }

    /// The Obs ID that was asked for, if it is one.
    pub fn obs_id(self) -> Option<ObsId> {
        match self {
            Self::Job(_) => None,
            Self::Obs(obs_id) => Some(obs_id),
        }
    }
}

/// The options of [`MwaAsvoClient::download_many`]: the fields of
/// [`DownloadOptions`] that are the same for every download, and callbacks
/// that the threads of the downloads share.
pub struct DownloadManyOptions<'a> {
    /// As [`DownloadOptions::keep_tar`].
    pub keep_tar: bool,
    /// As [`DownloadOptions::no_resume`].
    pub no_resume: bool,
    /// As [`DownloadOptions::hash`].
    pub hash: bool,
    /// As [`DownloadOptions::download_dir`].
    pub download_dir: &'a Path,
    /// As [`DownloadOptions::job_subdirectory`].
    pub job_subdirectory: bool,
    /// As [`DownloadOptions::buffer_size`].
    pub buffer_size: usize,
    /// As [`DownloadOptions::retry_duration`].
    pub retry_duration: Duration,
    /// How many downloads run at the same time. See
    /// [`DEFAULT_CONCURRENT_DOWNLOADS`].
    pub concurrency: NonZeroUsize,
    /// Called with the index of the download in the result (from 0) and each
    /// [`DownloadProgress`] event of it, from the thread of the download.
    pub progress: Option<&'a (dyn Fn(usize, DownloadProgress) + Sync)>,
    /// As [`DownloadOptions::should_stop`], for every download.
    pub should_stop: Option<&'a (dyn Fn() -> bool + Sync)>,
}

impl<'a> DownloadManyOptions<'a> {
    /// The options of downloads to `download_dir`, with the defaults of
    /// [`DownloadOptions::new`], [`DEFAULT_CONCURRENT_DOWNLOADS`] downloads at
    /// the same time, and no `progress` and `should_stop` callbacks.
    ///
    /// Change a field after the call to use a different value.
    pub fn new(download_dir: &'a Path) -> Self {
        let one = DownloadOptions::new(download_dir);
        Self {
            keep_tar: one.keep_tar,
            no_resume: one.no_resume,
            hash: one.hash,
            download_dir: one.download_dir,
            job_subdirectory: one.job_subdirectory,
            buffer_size: one.buffer_size,
            retry_duration: one.retry_duration,
            concurrency: DEFAULT_CONCURRENT_DOWNLOADS,
            progress: None,
            should_stop: None,
        }
    }

    /// The [`DownloadOptions`] of download `number` (from 1) of `count`.
    fn one<'b>(
        &'b self,
        number: usize,
        count: usize,
        progress: Option<&'b dyn Fn(DownloadProgress)>,
        should_stop: Option<&'b dyn Fn() -> bool>,
    ) -> DownloadOptions<'b> {
        DownloadOptions {
            keep_tar: self.keep_tar,
            no_resume: self.no_resume,
            hash: self.hash,
            download_dir: self.download_dir,
            job_subdirectory: self.job_subdirectory,
            progress,
            download_number: number,
            download_count: count,
            buffer_size: self.buffer_size,
            retry_duration: self.retry_duration,
            should_stop,
        }
    }
}

/// One result of [`MwaAsvoClient::download_many`]: the target, and the job
/// that was downloaded or the error of the download.
pub type DownloadManyResult = (DownloadTarget, Result<MwaAsvoJob, MwaAsvoError>);

impl MwaAsvoClient {
    /// Download each of `targets`, [`DownloadManyOptions::concurrency`] at a
    /// time, as `giant-squid download` does.
    ///
    /// The client gets the job list once for all the downloads (a Job ID
    /// that is not in it, because it is older than the list's window, is
    /// got by its ID, as [`Self::download_job`] does). A target that is given
    /// more than once is downloaded once, and so is an Obs ID whose job is
    /// also given by its Job ID: the two would write the same files at the
    /// same time. The downloads start in the order of the targets. Every
    /// download runs, whatever the others do.
    ///
    /// # Errors
    ///
    /// [`MwaAsvoError::InvalidOption`] for an option that cannot be used,
    /// and [`MwaAsvoError::Api`] if the job list request fails: then nothing
    /// is downloaded. Otherwise, the result has the targets that were
    /// downloaded, in their order, each with its own result.
    pub fn download_many(
        &self,
        targets: &[DownloadTarget],
        opts: &DownloadManyOptions,
    ) -> Result<Vec<DownloadManyResult>, MwaAsvoError> {
        opts.one(1, 1, None, None).check()?; // before the job list request
        let jobs = self.get_jobs(&JobsFilter::default())?;
        let targets = without_same_jobs(without_repeats(targets), &jobs);
        let count = targets.len();
        let results = run_in_order(
            opts.concurrency,
            &targets,
            DOWNLOAD_START_DELAY,
            |index, target| {
                let progress = |event| {
                    if let Some(progress) = opts.progress {
                        progress(index, event);
                    }
                };
                let should_stop = || opts.should_stop.is_some_and(|should_stop| should_stop());
                let one = opts.one(index + 1, count, Some(&progress), Some(&should_stop));
                self.download_target(&jobs, *target, &one)
            },
        );
        Ok(targets.into_iter().zip(results).collect())
    }

    /// Download `target`, found in the job list `jobs`, or by its Job ID if
    /// it is not there.
    fn download_target(
        &self,
        jobs: &MwaAsvoJobVec,
        target: DownloadTarget,
        opts: &DownloadOptions,
    ) -> Result<MwaAsvoJob, MwaAsvoError> {
        match target {
            DownloadTarget::Job(job_id) if jobs.0.iter().any(|j| j.job_id() == job_id) => {
                self.download_job_from(jobs, job_id, opts)
            }
            DownloadTarget::Job(job_id) => self.download_job(job_id, opts),
            DownloadTarget::Obs(obs_id) => self.download_obs_from(jobs, obs_id, opts),
        }
    }
}

/// `targets` with each target once, at its first place.
fn without_repeats(targets: &[DownloadTarget]) -> Vec<DownloadTarget> {
    let mut unique = Vec::with_capacity(targets.len());
    for target in targets {
        if !unique.contains(target) {
            unique.push(*target);
        }
    }
    unique
}

/// `targets` without each Obs ID whose job (in `jobs`) is also a Job ID of
/// `targets`. The two would download the same job into the same directory at
/// the same time. Each Obs ID that is removed is logged. An Obs ID with no job
/// to download is kept, so that its error is reported.
fn without_same_jobs(targets: Vec<DownloadTarget>, jobs: &MwaAsvoJobVec) -> Vec<DownloadTarget> {
    let job_ids: Vec<MwaAsvoJobId> = targets.iter().filter_map(|t| t.job_id()).collect();
    targets
        .into_iter()
        .filter(|target| {
            let DownloadTarget::Obs(obs_id) = *target else {
                return true;
            };
            match ready_job_of_obs_id(jobs, obs_id) {
                Ok(job) if job_ids.contains(&job.job_id()) => {
                    let job_id = job.job_id();
                    info!("Obs ID {obs_id} is Job ID {job_id}, which is also given: it is downloaded once.");
                    false
                }
                _ => true,
            }
        })
        .collect()
}

/// Run `work` for each of `items` on `threads` threads, and give the results
/// in the order of the items.
///
/// The items start in their order: a thread that is free takes the next item
/// that has not started, and the starts are `start_delay` apart (for the
/// display: the first log line of an item comes before the start of the
/// next). `work` is given the index of the item, from 0.
pub(crate) fn run_in_order<T, R>(
    threads: NonZeroUsize,
    items: &[T],
    start_delay: Duration,
    work: impl Fn(usize, &T) -> R + Sync,
) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    // The index of the next item to start. It is held for the wait, so that
    // another thread cannot start the next item before this one has.
    let next = Mutex::new(0_usize);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    thread::scope(|scope| {
        for _ in 0..threads.get().min(items.len()) {
            scope.spawn(|| loop {
                let index = {
                    let mut next = next.lock().unwrap_or_else(PoisonError::into_inner);
                    if *next >= items.len() {
                        return;
                    }
                    let index = *next;
                    *next += 1;
                    thread::sleep(start_delay);
                    index
                };
                let result = work(index, &items[index]);
                done.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((index, result));
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, result)| result).collect()
}
