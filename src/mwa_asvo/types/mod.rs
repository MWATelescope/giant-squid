// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use jiff::Timestamp;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::mwa_asvo::api::openapi::{
    JobDetailResponse, JobFile, JobState, JobType, Type as FileType,
};
use crate::mwa_asvo::api::schema_enums::SchemaEnum;
use crate::{obs_id::ObsId, AsvoError};

/// The query key of the time at which a signed download URL stops working:
/// seconds since the Unix epoch (an S3 presigned URL has `Expires=<seconds>`).
const URL_EXPIRES_KEY: &str = "Expires";

/// How long a signed download URL works, in days, from the time the MWA ASVO
/// signs it. The URL has the end of that time in its `Expires`.
pub const SIGNED_URL_LIFETIME_DAYS: u32 = 7;

/// What [`AsvoJob::display_state_at`] adds to `completed` for a job whose
/// download links work, and for a job whose links have expired.
const DISPLAY_READY: &str = "ready";
const DISPLAY_EXPIRED: &str = "expired";

/// The time at which the signed download URL `url` stops working: its
/// `Expires` query value. `None` if `url` is not a URL, or has no usable
/// `Expires`.
pub(crate) fn signed_url_expiry(url: &str) -> Option<Timestamp> {
    let url = reqwest::Url::parse(url).ok()?;
    let (_, value) = url.query_pairs().find(|(key, _)| key == URL_EXPIRES_KEY)?;
    Timestamp::from_second(value.parse::<i64>().ok()?).ok()
}

impl JobType {
    /// The names of the job types: the schema's values (for example
    /// `visibility`), in the schema's order. [`JobType::parse_name`] accepts
    /// each of them.
    pub fn names() -> Vec<String> {
        <Self as SchemaEnum>::names()
    }

    /// Parse the name of a job type, as a user types it (see
    /// [`SchemaEnum::from_name`]): the case, spaces, hyphens and underscores
    /// do not matter.
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidJobType`] for any other text.
    pub fn parse_name(s: &str) -> Result<Self, AsvoError> {
        Self::from_name(s).ok_or_else(|| AsvoError::InvalidJobType { str: s.to_string() })
    }
}

impl JobState {
    /// The names of the job states: the schema's values (for example
    /// `completed`), in the schema's order. [`JobState::parse_name`]
    /// accepts each of them.
    pub fn names() -> Vec<String> {
        <Self as SchemaEnum>::names()
    }

    /// Parse the name of a job state, as a user types it (see
    /// [`SchemaEnum::from_name`]): the case, spaces, hyphens and underscores
    /// do not matter.
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidJobState`] for any other text.
    pub fn parse_name(s: &str) -> Result<Self, AsvoError> {
        Self::from_name(s).ok_or_else(|| AsvoError::InvalidJobState { str: s.to_string() })
    }
}

impl JobFile {
    /// The size of the file in bytes. The schema types `size` as a signed
    /// integer; a negative size (which the MWA ASVO does not send) is 0.
    pub fn size_bytes(&self) -> u64 {
        u64::try_from(self.size).unwrap_or_default()
    }
}

/// An MWA ASVO Job ID: a `NonZeroU64`, as the OpenAPI schema's `job_id` is
/// (in `JobSubmittedResponse`, `JobDetailResponse` and the other responses).
/// A type alias, not a newtype, because a newtype would add complexity for no
/// gain.
pub type AsvoJobId = std::num::NonZeroU64;

/// An MWA ASVO job: the schema's `JobDetailResponse`, with the job's Obs ID
/// checked.
///
/// The fields of the `JobDetailResponse` are reached through [`Deref`]
/// (`job.job_id`, `job.job_state`, `job.product` and so on). The checked
/// Obs ID is [`AsvoJob::obs_id`].
///
/// In JSON the job is its `JobDetailResponse`, as the API gives it.
///
/// [`Deref`]: std::ops::Deref
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AsvoJob {
    /// `detail.obs_id`, checked. Not in the JSON: that has `obs_id`.
    #[serde(skip)]
    obs_id: ObsId,
    #[serde(flatten)]
    detail: JobDetailResponse,
}

impl AsvoJob {
    /// The job's Obs ID: the `obs_id` of its `JobDetailResponse`, checked.
    pub fn obs_id(&self) -> ObsId {
        self.obs_id
    }

    /// The job's ID: the `job_id` of its `JobDetailResponse`.
    pub fn job_id(&self) -> AsvoJobId {
        self.detail.job_id
    }

    /// The job as the API gives it.
    pub fn detail(&self) -> &JobDetailResponse {
        &self.detail
    }

    /// The job as the API gives it, to change in a test. A test must not
    /// change `obs_id`.
    #[cfg(test)]
    pub(crate) fn detail_mut(&mut self) -> &mut JobDetailResponse {
        &mut self.detail
    }
}

impl AsvoJob {
    /// When the download links of the job stop working: the earliest
    /// `Expires` of the signed URLs of its Acacia files. The MWA ASVO signs
    /// them for [`SIGNED_URL_LIFETIME_DAYS`] days. `None` if the job has no
    /// Acacia file with a URL that has a usable `Expires` (a Scratch or DUG
    /// job, or a job with no product yet).
    pub fn expires(&self) -> Option<Timestamp> {
        self.detail
            .product
            .as_ref()?
            .files
            .iter()
            .filter(|file| file.type_ == FileType::Acacia)
            .filter_map(|file| file.url.as_deref().and_then(signed_url_expiry))
            .min()
    }

    /// Whether the job is completed and its download links have expired at
    /// `now`: a download of it would be refused. The API does not say so (it
    /// has no such state), so this is worked out from the URLs. A job with no
    /// readable expiry is not expired.
    pub fn is_expired_at(&self, now: Timestamp) -> bool {
        self.detail.job_state == JobState::Completed && self.expires().is_some_and(|e| e <= now)
    }

    /// [`Self::is_expired_at`] for the current time.
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Timestamp::now())
    }

    /// The job state as `giant-squid list` and `wait` show it, at `now`: the
    /// API's state (for example `queued`), except that a completed job is
    /// `completed (ready)` if its download links work and `completed
    /// (expired)` if they have expired. This is for display only: the job's
    /// `job_state` is the API's.
    pub fn display_state_at(&self, now: Timestamp) -> String {
        let state = self.detail.job_state;
        if state != JobState::Completed {
            return state.to_string();
        }
        let links = if self.is_expired_at(now) {
            DISPLAY_EXPIRED
        } else {
            DISPLAY_READY
        };
        format!("{state} ({links})")
    }

    /// [`Self::display_state_at`] for the current time.
    pub fn display_state(&self) -> String {
        self.display_state_at(Timestamp::now())
    }
}

impl std::ops::Deref for AsvoJob {
    type Target = JobDetailResponse;

    fn deref(&self) -> &JobDetailResponse {
        &self.detail
    }
}

/// Check a job from the API: its `obs_id` must be a valid Obs ID.
///
/// # Errors
///
/// [`AsvoError::InvalidJob`], with what is wrong.
impl TryFrom<JobDetailResponse> for AsvoJob {
    type Error = AsvoError;

    fn try_from(detail: JobDetailResponse) -> Result<Self, AsvoError> {
        let invalid = |problem: String| AsvoError::InvalidJob {
            id: detail.job_id,
            problem,
        };
        let obs_id = u64::try_from(detail.obs_id)
            .map_err(|_| invalid(format!("its obs_id is negative ({})", detail.obs_id)))?;
        let obs_id =
            ObsId::validate(obs_id).map_err(|e| invalid(format!("its obs_id is invalid: {e}")))?;
        Ok(Self { obs_id, detail })
    }
}

/// A vector of ASVO jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Clone, Debug, PartialEq)]
pub struct AsvoJobVec(pub Vec<AsvoJob>);

impl AsvoJobVec {
    /// The jobs in JSON form: an object keyed by Job ID, in Job ID order,
    /// whose values have the keys shown on [`AsvoJob`]. `giant-squid list
    /// --json` prints this.
    ///
    /// If the job list has a Job ID more than once, the output has only the
    /// last of those jobs.
    pub fn json(self) -> Result<String, serde_json::Error> {
        let map: BTreeMap<AsvoJobId, AsvoJob> =
            self.0.into_iter().map(|j| (j.job_id(), j)).collect();
        serde_json::to_string(&map)
    }

    /// Check whether all of `job_ids` are ready for download, in this job
    /// list. This makes no request: to wait for jobs, the caller gets the
    /// job list ([`AsvoClient::get_jobs`](crate::AsvoClient::get_jobs)),
    /// calls this, and sleeps and repeats while it returns `Ok(false)`.
    ///
    /// Returns `Ok(true)` if every job is `Completed`, and `Ok(false)` if
    /// every job is completed or still in progress (queued, processing and
    /// so on). Returns an error for the first job (in the order of
    /// `job_ids`) that is not in the list ([`AsvoError::NoAsvoJob`]), has an
    /// error ([`AsvoError::JobFailed`]), has been cancelled
    /// ([`AsvoError::JobCancelled`]) or is completed but has download links
    /// that have expired ([`AsvoError::JobExpired`]: waiting longer does not
    /// help, the job has to be resubmitted).
    pub fn all_ready(&self, job_ids: &[AsvoJobId]) -> Result<bool, AsvoError> {
        self.all_ready_at(job_ids, Timestamp::now())
    }

    /// [`Self::all_ready`] at the time `now`.
    pub fn all_ready_at(&self, job_ids: &[AsvoJobId], now: Timestamp) -> Result<bool, AsvoError> {
        let mut all_ready = true;
        for job_id in job_ids {
            let job = self
                .0
                .iter()
                .find(|j| j.job_id() == *job_id)
                .ok_or(AsvoError::NoAsvoJob(*job_id))?;
            match &job.job_state {
                JobState::Completed if job.is_expired_at(now) => {
                    return Err(AsvoError::JobExpired {
                        job_id: *job_id,
                        expired: job.expires(),
                    });
                }
                JobState::Completed => (),
                JobState::Error => {
                    return Err(AsvoError::JobFailed {
                        job_id: *job_id,
                        obs_id: job.obs_id,
                        error: job.error_text.clone().unwrap_or_default(),
                        error_code: job.error_code,
                    });
                }
                JobState::Cancelled => return Err(AsvoError::JobCancelled(*job_id)),
                _ => all_ready = false,
            }
        }
        Ok(all_ready)
    }

    /// Keep only the jobs that match every non-empty filter. An empty slice
    /// does not filter.
    ///
    /// - `job_ids`: the Job ID is one of these.
    /// - `obs_ids`: the Obs ID is one of these.
    /// - `job_types`: the job type is one of these. A job with no type does
    ///   not match.
    /// - `states`: the job state is one of these.
    pub fn filter(
        self,
        job_ids: &[AsvoJobId],
        obs_ids: &[ObsId],
        job_types: &[JobType],
        states: &[JobState],
    ) -> Self {
        self.retain(|j| {
            (job_ids.is_empty() || job_ids.contains(&j.job_id()))
                && (obs_ids.is_empty() || obs_ids.contains(&j.obs_id))
                && (job_types.is_empty() || j.job_type.is_some_and(|t| job_types.contains(&t)))
                && (states.is_empty() || states.contains(&j.job_state))
        })
    }

    /// Keep only the jobs for which `predicate` returns `true`.
    pub fn retain(mut self, predicate: impl Fn(&AsvoJob) -> bool) -> Self {
        self.0.retain(predicate);
        self
    }
}

/// A short description of the job, for a message: its ID, Obs ID, type,
/// state and files.
impl std::fmt::Display for AsvoJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Job ID: {job_id}, Obs ID: {obs_id}, type: {type}, state: {state}, files: {files:?}",
            obs_id=self.obs_id,
            job_id=self.job_id(),
            type=self.job_type.map(|t| t.to_string()).unwrap_or_default(),
            state=self.job_state,
            files=self.product.as_ref().map(|p| &p.files),
        )
    }
}

#[cfg(test)]
mod tests;
