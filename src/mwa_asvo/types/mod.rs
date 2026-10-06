// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use serde::Serialize;
use std::collections::BTreeMap;

use crate::mwa_asvo::api::openapi::{JobDetailResponse, JobFile, JobState, JobType};
use crate::mwa_asvo::api::schema_enums::SchemaEnum;
use crate::{obs_id::ObsId, AsvoError};

/// Sanitize a string to lowercase, and ascii 'a'-'z' only.
///
/// Used to sanitize user input for ASVO identifiers.
fn _sanitize_identifier(s: &str) -> String {
    let mut sanitized = s.to_lowercase();
    sanitized.retain(|c| c.is_ascii_lowercase());
    sanitized
}

/// The job types of the OpenAPI schema: each `JobType` code, with the name
/// that the schema gives it. The schema's `JobType` is only an integer; the
/// names are in the description of the `job_type` fields ("0=conversion,
/// 1=visibility, ..."). A test checks this table against the schema.
const JOB_TYPE_NAMES: [(i64, &str); 7] = [
    (0, "conversion"),
    (1, "visibility"),
    (2, "metadata"),
    (3, "voltage"),
    (4, "cancel"),
    (5, "beamformer"),
    (6, "imaging"),
];

impl JobType {
    /// The name of the job type, for example `visibility`.
    pub fn name(&self) -> &'static str {
        JOB_TYPE_NAMES
            .iter()
            .find(|(code, _)| *code == **self)
            .map(|(_, name)| *name)
            .expect("every JobType code of the schema is in JOB_TYPE_NAMES")
    }

    /// The names of the job types, in the order of their codes.
    /// [`JobType::parse_name`] accepts each of them.
    pub fn names() -> Vec<&'static str> {
        JOB_TYPE_NAMES.iter().map(|(_, name)| *name).collect()
    }

    /// Parse the name of a job type, as a user types it: the case, spaces,
    /// hyphens and underscores do not matter.
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidJobType`] for any other text.
    pub fn parse_name(s: &str) -> Result<Self, AsvoError> {
        let wanted = _sanitize_identifier(s);
        JOB_TYPE_NAMES
            .iter()
            .find(|(_, name)| _sanitize_identifier(name) == wanted)
            .and_then(|(code, _)| JobType::try_from(*code).ok())
            .ok_or_else(|| AsvoError::InvalidJobType { str: s.to_string() })
    }
}

/// The name of the job type (see [`JobType::name`]).
impl std::fmt::Display for JobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl JobState {
    /// The names of the job states: the schema's values (for example
    /// `completed`), in the schema's order. [`JobState::parse_name`]
    /// accepts each of them.
    pub fn names() -> Vec<String> {
        JobState::VARIANTS.iter().map(ToString::to_string).collect()
    }

    /// Parse the name of a job state, as a user types it: the case,
    /// spaces, hyphens and underscores do not matter, so `WAIT-CAL` and
    /// `waitcal` are the same state. (The schema's own `FromStr` accepts
    /// only the exact value.)
    ///
    /// # Errors
    ///
    /// [`AsvoError::InvalidJobState`] for any other text.
    pub fn parse_name(s: &str) -> Result<Self, AsvoError> {
        let wanted = _sanitize_identifier(s);
        JobState::VARIANTS
            .iter()
            .find(|state| _sanitize_identifier(&state.to_string()) == wanted)
            .copied()
            .ok_or_else(|| AsvoError::InvalidJobState { str: s.to_string() })
    }
}

impl JobFile {
    /// The size of the file in bytes. The schema types `size` as a signed
    /// integer; a negative size (which the MWA ASVO does not send) is 0.
    pub fn size_bytes(&self) -> u64 {
        u64::try_from(self.size).unwrap_or_default()
    }
}

/// An MWA ASVO job ID: a `NonZeroU64`, as the OpenAPI schema's `job_id` is
/// (in `JobSubmittedResponse` and the other responses). The `id` of a
/// `JobDetailResponse` is an `i64`; [`AsvoJob`] checks it. A type alias, not
/// a newtype, because a newtype would add complexity for no gain.
pub type AsvoJobId = std::num::NonZeroU64;

/// An MWA ASVO job: the schema's `JobDetailResponse`, with the job's obsid
/// and ID checked.
///
/// The fields of the `JobDetailResponse` are reached through [`Deref`]
/// (`job.job_state`, `job.product` and so on). The obsid, which the API has
/// only in the untyped `job_params`, is [`AsvoJob::obs_id`], and the job ID
/// is [`AsvoJob::job_id`].
///
/// In JSON the job is its `JobDetailResponse`, as the API gives it, with one
/// more key, `obs_id`.
///
/// [`Deref`]: std::ops::Deref
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AsvoJob {
    obs_id: ObsId,
    /// `detail.id`, checked. Not in the JSON: that has `id`.
    #[serde(skip)]
    job_id: AsvoJobId,
    #[serde(flatten)]
    detail: JobDetailResponse,
}

impl AsvoJob {
    /// The job's obsid, from its `job_params`.
    pub fn obs_id(&self) -> ObsId {
        self.obs_id
    }

    /// The job's ID: the `id` of its `JobDetailResponse`.
    pub fn job_id(&self) -> AsvoJobId {
        self.job_id
    }

    /// The job as the API gives it.
    pub fn detail(&self) -> &JobDetailResponse {
        &self.detail
    }

    /// The job as the API gives it, to change in a test. A test must not
    /// change `id` or the `obs_id` of `job_params`.
    #[cfg(test)]
    pub(crate) fn detail_mut(&mut self) -> &mut JobDetailResponse {
        &mut self.detail
    }
}

impl std::ops::Deref for AsvoJob {
    type Target = JobDetailResponse;

    fn deref(&self) -> &JobDetailResponse {
        &self.detail
    }
}

/// Check a job from the API: its `id` must be a job ID, and its
/// `job_params` must have a valid `obs_id` (a number, or a string of
/// digits, as the MWA ASVO has sent both).
///
/// # Errors
///
/// [`AsvoError::InvalidJob`], with what is wrong.
impl TryFrom<JobDetailResponse> for AsvoJob {
    type Error = AsvoError;

    fn try_from(detail: JobDetailResponse) -> Result<Self, AsvoError> {
        let invalid = |problem: String| AsvoError::InvalidJob {
            id: detail.id,
            problem,
        };
        let job_id = u64::try_from(detail.id)
            .ok()
            .and_then(AsvoJobId::new)
            .ok_or_else(|| invalid("its ID is not a job ID".to_string()))?;
        let obs_id = detail
            .job_params
            .get("obs_id")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
            .ok_or_else(|| invalid("its job_params have no usable obs_id".to_string()))?;
        let obs_id = ObsId::validate(obs_id)
            .map_err(|e| invalid(format!("its job_params have an invalid obs_id: {e}")))?;
        Ok(Self {
            obs_id,
            job_id,
            detail,
        })
    }
}

/// A vector of ASVO jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Clone, Debug, PartialEq)]
pub struct AsvoJobVec(pub Vec<AsvoJob>);

impl AsvoJobVec {
    /// Get a vector of ASVO jobs in JSON form: an object keyed by job ID,
    /// whose values have the keys shown on [`AsvoJob`]. `giant-squid list
    /// --json` prints this.
    ///
    /// If the situation should arise that your job listing has an ASVO job ID
    /// more than once, only one of them will be visible in the output of this
    /// method!
    pub fn json(self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&AsvoJobMap::from(self).0)
    }

    /// Convert the vector to a map.
    ///
    /// If the situation should arise that your job listing has an ASVO job ID
    /// more than once, only one of them will be visible in the output of this
    /// method!
    pub fn into_map(self) -> AsvoJobMap {
        AsvoJobMap::from(self)
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
    /// error ([`AsvoError::JobFailed`]) or has been cancelled
    /// ([`AsvoError::JobCancelled`]).
    pub fn all_ready(&self, job_ids: &[AsvoJobId]) -> Result<bool, AsvoError> {
        let mut all_ready = true;
        for job_id in job_ids {
            let job = self
                .0
                .iter()
                .find(|j| j.job_id == *job_id)
                .ok_or(AsvoError::NoAsvoJob(*job_id))?;
            match &job.job_state {
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
    /// - `job_ids`: the job ID is one of these.
    /// - `obs_ids`: the obsid is one of these.
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
            (job_ids.is_empty() || job_ids.contains(&j.job_id))
                && (obs_ids.is_empty() || obs_ids.contains(&j.obs_id))
                && (job_types.is_empty() || j.job_type.is_some_and(|t| job_types.contains(&t)))
                && (states.is_empty() || states.contains(&j.job_state))
        })
    }

    /// filter out any jobs that don't match jobids
    pub fn retain(mut self, predicate: impl Fn(&AsvoJob) -> bool) -> Self {
        // if we wanted to use a nightly:
        // self.0.drain_filter(|j| predicate);
        self.0.retain(predicate);
        self
    }
}

/// A `BTreeMap` of ASVO job IDs against their jobs. Useful for efficiently
/// isolating specific jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Serialize, Debug, PartialEq)]
pub struct AsvoJobMap(pub BTreeMap<AsvoJobId, AsvoJob>);

impl From<AsvoJobVec> for AsvoJobMap {
    fn from(job_vec: AsvoJobVec) -> AsvoJobMap {
        let mut tree = BTreeMap::new();
        for j in job_vec.0.into_iter() {
            tree.insert(j.job_id, j);
        }
        AsvoJobMap(tree)
    }
}

// Boring Display methods.
impl std::fmt::Display for AsvoJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Job ID: {job_id}, obsid: {obs_id}, type: {type}, state: {state}, product_array: {files:?}",
            obs_id=self.obs_id,
            job_id=self.job_id,
            type=self.job_type.map(|t| t.name()).unwrap_or_default(),
            state=self.job_state,
            files=self.product.as_ref().map(|p| &p.files),
        )
    }
}

#[cfg(test)]
mod tests;
