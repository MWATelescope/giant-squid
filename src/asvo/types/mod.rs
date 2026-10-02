// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use jiff::Timestamp;
use serde::Serialize;
use std::{collections::BTreeMap, str::FromStr};

use crate::{obs_id::ObsId, AsvoError};

/// Sanitize a string to lowercase, and ascii 'a'-'z' only.
///
/// Used to sanitize user input for ASVO identifiers.
fn _sanitize_identifier(s: &str) -> String {
    let mut sanitized = s.to_lowercase();
    sanitized.retain(|c| c.is_ascii_lowercase());
    sanitized
}

/// All of the available types of ASVO jobs.
#[derive(Serialize, PartialEq, Eq, Debug, Clone, Copy)]
pub enum AsvoJobType {
    Conversion,
    DownloadVisibilities,
    DownloadMetadata,
    DownloadVoltage,
    CancelJob,
    DownloadBeamformer,
    Imaging,
    Unknown,
}

/// Parses the name of a job type: the case, spaces, hyphens and underscores
/// do not matter, so `download_visibilities` and `DownloadVisibilities` are
/// the same type. `download_voltage` and `download_voltages` both give
/// [`AsvoJobType::DownloadVoltage`].
///
/// # Errors
///
/// [`AsvoError::InvalidJobType`] for any other text, including "unknown":
/// [`AsvoJobType::Unknown`] stands for a job type that the MWA ASVO has and
/// this version does not, so it has no name to ask for. (Before 3.0.0 any
/// other text gave `Unknown`, so a misspelt name in `list --job-types`
/// quietly matched no job.)
impl FromStr for AsvoJobType {
    type Err = AsvoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match _sanitize_identifier(s).as_str() {
            "conversion" => Ok(AsvoJobType::Conversion),
            "downloadvisibilities" => Ok(AsvoJobType::DownloadVisibilities),
            "downloadmetadata" => Ok(AsvoJobType::DownloadMetadata),
            "downloadvoltage" | "downloadvoltages" => Ok(AsvoJobType::DownloadVoltage),
            "downloadbeamformer" => Ok(AsvoJobType::DownloadBeamformer),
            "canceljob" => Ok(AsvoJobType::CancelJob),
            "imaging" => Ok(AsvoJobType::Imaging),
            _ => Err(AsvoError::InvalidJobType { str: s.to_string() }),
        }
    }
}

/// All of states an ASVO job may be in.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub enum AsvoJobState {
    Queued,
    WaitCal,
    Staging,
    Staged,
    Preparing,
    Downloading,
    Preprocessing,
    Imaging,
    Delivering,
    Ready, // aka Completed
    Error(String),
    Expired,
    Cancelled,
}

impl FromStr for AsvoJobState {
    type Err = AsvoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match _sanitize_identifier(s).as_str() {
            "queued" => Ok(AsvoJobState::Queued),
            "waitcal" => Ok(AsvoJobState::WaitCal),
            "staging" => Ok(AsvoJobState::Staging),
            "staged" => Ok(AsvoJobState::Staged),
            "downloading" => Ok(AsvoJobState::Downloading),
            "preparing" => Ok(AsvoJobState::Preparing),
            "preprocessing" => Ok(AsvoJobState::Preprocessing),
            "imaging" => Ok(AsvoJobState::Imaging),
            "delivering" => Ok(AsvoJobState::Delivering),
            "ready" => Ok(AsvoJobState::Ready),
            "error" => Ok(AsvoJobState::Error(String::new())),
            "expired" => Ok(AsvoJobState::Expired),
            "cancelled" => Ok(AsvoJobState::Cancelled),
            _ => Err(AsvoError::InvalidJobState { str: s.to_string() }),
        }
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
    pub job_type: AsvoJobType,
    pub job_state: AsvoJobState,
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
    /// The server's error message, or `None`. For a job in the `Error`
    /// state it is also in [`AsvoJobState::Error`].
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
    /// Returns `Ok(true)` if every job is `Ready`, and `Ok(false)` if every
    /// job is ready or still in progress (queued, processing and so on).
    /// Returns an error for the first job (in the order of `job_ids`) that
    /// is not in the list ([`AsvoError::NoAsvoJob`]), has an error
    /// ([`AsvoError::JobFailed`]), has expired ([`AsvoError::JobExpired`])
    /// or has been cancelled ([`AsvoError::JobCancelled`]).
    pub fn all_ready(&self, job_ids: &[AsvoJobId]) -> Result<bool, AsvoError> {
        let mut all_ready = true;
        for job_id in job_ids {
            let job = self
                .0
                .iter()
                .find(|j| j.job_id == *job_id)
                .ok_or(AsvoError::NoAsvoJob(*job_id))?;
            match &job.job_state {
                AsvoJobState::Ready => (),
                AsvoJobState::Error(e) => {
                    return Err(AsvoError::JobFailed {
                        job_id: *job_id,
                        obs_id: job.obs_id,
                        error: e.clone(),
                        error_code: job.error_code,
                    });
                }
                AsvoJobState::Expired => return Err(AsvoError::JobExpired(*job_id)),
                AsvoJobState::Cancelled => return Err(AsvoError::JobCancelled(*job_id)),
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
    /// - `job_types`: the job type is one of these.
    /// - `states`: the job state is one of these. Only the kind of state is
    ///   compared, so any `AsvoJobState::Error(..)` matches every other.
    pub fn filter(
        self,
        job_ids: &[AsvoJobId],
        obs_ids: &[ObsId],
        job_types: &[AsvoJobType],
        states: &[AsvoJobState],
    ) -> Self {
        self.retain(|j| {
            (job_ids.is_empty() || job_ids.contains(&j.job_id))
                && (obs_ids.is_empty() || obs_ids.contains(&j.obs_id))
                && (job_types.is_empty() || job_types.contains(&j.job_type))
                && (states.is_empty()
                    || states
                        .iter()
                        .any(|s| std::mem::discriminant(s) == std::mem::discriminant(&j.job_state)))
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
impl std::fmt::Display for AsvoJobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AsvoJobType::Conversion => "Conversion",
                AsvoJobType::DownloadVisibilities => "Download Visibilities",
                AsvoJobType::DownloadMetadata => "Download Metadata",
                AsvoJobType::DownloadVoltage => "Download Voltage",
                AsvoJobType::DownloadBeamformer => "Download Beamformer",
                AsvoJobType::CancelJob => "Cancel Job",
                AsvoJobType::Imaging => "Imaging",
                AsvoJobType::Unknown => "Unknown",
            }
        )
    }
}

impl std::fmt::Display for AsvoJobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AsvoJobState::Queued => "Queued".to_string(),
                AsvoJobState::WaitCal => "Waiting for calibration solution".to_string(),
                AsvoJobState::Staging => "Staging".to_string(),
                AsvoJobState::Staged => "Staged".to_string(),
                AsvoJobState::Preparing => "Preparing".to_string(),
                AsvoJobState::Downloading => "Retrieving from archive".to_string(),
                AsvoJobState::Preprocessing => "Preprocessing".to_string(),
                AsvoJobState::Imaging => "Imaging".to_string(),
                AsvoJobState::Delivering => "Delivering".to_string(),
                AsvoJobState::Ready => "Ready".to_string(),
                AsvoJobState::Error(e) => format!("Error: {}", e),
                AsvoJobState::Expired => "Expired".to_string(),
                AsvoJobState::Cancelled => "Cancelled".to_string(),
            },
        )
    }
}

impl std::fmt::Display for AsvoJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Job ID: {job_id}, obsid: {obs_id}, type: {type}, state: {state}, product_array: {files:?}",
            obs_id=self.obs_id,
            job_id=self.job_id,
            type=self.job_type,
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

/// A download progress event, given to [`DownloadOptions::progress`].
///
/// For each file, the library sends one or more `Started` events, zero or
/// more `Advanced` events, then one `Finished` event. A second `Started`
/// for the same file means the download restarted (for example, the server
/// did not honour a resume request), so the caller resets its count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadProgress {
    /// A file download starts, or starts again.
    Started {
        /// The MWA ASVO job ID.
        job_id: AsvoJobId,
        /// A human-readable label for the download, for example
        /// `Job ID 123 (obsid: 1234567890) [1/2]:`.
        label: String,
        /// The size of the file in bytes.
        total_bytes: u64,
        /// The number of bytes already on disk (non-zero for a resumed
        /// download).
        position: u64,
    },
    /// `bytes` more bytes were written.
    Advanced { bytes: u64 },
    /// The file download is complete, or was skipped because the file is
    /// already on disk.
    Finished,
}

/// Options common to all download operations.
pub struct DownloadOptions<'a> {
    pub keep_tar: bool,
    pub no_resume: bool,
    pub hash: bool,
    pub download_dir: &'a str,
    /// Called with each [`DownloadProgress`] event. `None` reports no
    /// progress. The library has no user interface of its own.
    pub progress: Option<&'a dyn Fn(DownloadProgress)>,
    pub download_number: usize,
    pub download_count: usize,
    /// How much data, in bytes, is held in memory before it is written to
    /// disk. See [`DEFAULT_DOWNLOAD_BUFFER_SIZE`](crate::DEFAULT_DOWNLOAD_BUFFER_SIZE).
    pub buffer_size: usize,
    /// How long to retry transient download failures before giving up.
    /// Zero disables retrying. See
    /// [`DEFAULT_DOWNLOAD_RETRY_DURATION`](crate::DEFAULT_DOWNLOAD_RETRY_DURATION).
    pub retry_duration: std::time::Duration,
    /// Asked between chunks of a download, and during the wait before a
    /// retry, whether to stop. When it returns `true`, the download stops
    /// with [`AsvoError::Interrupted`](crate::AsvoError::Interrupted), which
    /// is never retried. `None` never stops. The caller decides what a stop
    /// means (for example, Ctrl-C in the Python module); the library reads
    /// no signals. A file that is being written stays on disk, partial, so
    /// a later download can resume it.
    pub should_stop: Option<&'a dyn Fn() -> bool>,
}

#[cfg(test)]
mod tests;
