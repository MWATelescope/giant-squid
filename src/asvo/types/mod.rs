// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use jiff::Timestamp;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::asvo::apiv2::openapi::{JobState, JobType};
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

// The generated `JobType` derives only `Clone` and `Debug`. It is an
// integer, so it compares, hashes and copies as one.
impl PartialEq for JobType {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl Eq for JobType {}
impl Copy for JobType {}
impl std::hash::Hash for JobType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

/// Every job state of the OpenAPI schema, in the schema's order. The
/// help of `list --job-states` lists them in this order.
const JOB_STATES: [JobState; 12] = [
    JobState::Preparing,
    JobState::Queued,
    JobState::Waitcal,
    JobState::Staging,
    JobState::Staged,
    JobState::Downloading,
    JobState::Preprocessing,
    JobState::Imaging,
    JobState::Delivering,
    JobState::Completed,
    JobState::Error,
    JobState::Cancelled,
];

// A state that the schema adds is a compile error here, until it is added
// to `JOB_STATES` too.
const _: fn(JobState) = |state| match state {
    JobState::Preparing
    | JobState::Queued
    | JobState::Waitcal
    | JobState::Staging
    | JobState::Staged
    | JobState::Downloading
    | JobState::Preprocessing
    | JobState::Imaging
    | JobState::Delivering
    | JobState::Completed
    | JobState::Error
    | JobState::Cancelled => (),
};

impl JobState {
    /// The names of the job states: the schema's values (for example
    /// `completed`), in the schema's order. [`JobState::parse_name`]
    /// accepts each of them.
    pub fn names() -> Vec<String> {
        JOB_STATES.iter().map(ToString::to_string).collect()
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
        JOB_STATES
            .iter()
            .find(|state| _sanitize_identifier(&state.to_string()) == wanted)
            .copied()
            .ok_or_else(|| AsvoError::InvalidJobState { str: s.to_string() })
    }
}

/// A single file provided by an ASVO job.
///
/// In JSON the keys are the field names, which are the OpenAPI names of a
/// `JobFile`: `type`, `url`, `path`, `size`, `sha1` and `format`.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub struct AsvoFilesArray {
    /// Where the file is delivered.
    pub r#type: Delivery,
    pub url: Option<String>,
    pub path: Option<String>,
    pub size: u64,
    pub sha1: Option<String>,
    /// The file's format, as the MWA ASVO gives it, or `None`. The schema
    /// (v1.11) types it as a free string and does not document its values.
    pub format: Option<String>,
}

/// The product of a completed job: the OpenAPI `JobProduct`.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub struct AsvoJobProduct {
    /// The job's files. For a job from the server it is not empty: a
    /// product with no usable file is `None` on the job.
    pub files: Vec<AsvoFilesArray>,
}

/// An MWA ASVO job ID. A `u64`, as the OpenAPI schema's `job_id` is a
/// 64-bit integer. A type alias, not a newtype, because a newtype would add
/// complexity for no gain.
pub type AsvoJobId = u64;

/// All of the metadata associated with an ASVO job.
///
/// In JSON the keys are the field names, which are the OpenAPI names of a
/// `JobDetailResponse`. One differs in form: `obs_id`, which the API has
/// only in `job_params`, is also a field of its own here.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub struct AsvoJob {
    pub obs_id: ObsId,
    pub job_id: AsvoJobId,
    /// The job's type, or `None` if the server gives none.
    pub job_type: Option<JobType>,
    /// The job's state, the schema's `JobState`. For a job in the `Error`
    /// state, the message is [`AsvoJob::error_text`].
    pub job_state: JobState,
    /// The job's product (its files), or `None` if the job has none yet.
    pub product: Option<AsvoJobProduct>,
    /// When the job was created (UTC).
    pub created: Timestamp,
    /// When the job started, or `None` if it has not started.
    pub started: Option<Timestamp>,
    pub completed: Option<Timestamp>,
    /// When the job was last changed, or `None`.
    pub modified: Option<Timestamp>,
    /// The server's error code, or `None`. The schema gives it as an
    /// integer and does not document its values, so this library passes it
    /// on and does not interpret it. It is also in the message of
    /// [`AsvoError::JobFailed`].
    pub error_code: Option<i64>,
    /// The server's error message, or `None`.
    pub error_text: Option<String>,
    /// The ID of the user who submitted the job.
    pub user_id: i64,
    /// The first name of the user who submitted the job.
    pub first_name: String,
    /// The last name of the user who submitted the job.
    pub last_name: String,
    /// The job's parameters as the server gives them (`obs_id`, delivery,
    /// processing options), untyped because they differ by job type.
    pub job_params: serde_json::Map<String, serde_json::Value>,
}

/// A vector of ASVO jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Clone, Debug)]
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
#[derive(Serialize, PartialEq, Eq, Debug)]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum Delivery {
    /// "Deliver" the ASVO job to "the cloud" so it can be downloaded from
    /// anywhere.
    Acacia,

    /// Delivert the ASVO job to the filesystem on DUG (Curtin University's account)
    Dug,

    /// Deliver the ASVO job to the /scratch filesystem at the Pawsey
    /// Supercomputing Centre.
    Scratch,
}

impl std::fmt::Display for Delivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Delivery::Acacia => "acacia",
                Delivery::Dug => "dug",
                Delivery::Scratch => "scratch",
            }
        )
    }
}

#[cfg(test)]
mod tests;
