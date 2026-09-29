// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::{collections::BTreeMap, str::FromStr};

use crate::{obsid::Obsid, AsvoError};

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

impl FromStr for AsvoJobType {
    type Err = AsvoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match _sanitize_identifier(s).as_str() {
            "conversion" => Ok(AsvoJobType::Conversion),
            "downloadvisibilities" => Ok(AsvoJobType::DownloadVisibilities),
            "downloadmetadata" => Ok(AsvoJobType::DownloadMetadata),
            "downloadvoltages" => Ok(AsvoJobType::DownloadVoltage),
            "downloadbeamformer" => Ok(AsvoJobType::DownloadBeamformer),
            "canceljob" => Ok(AsvoJobType::CancelJob),
            "imaging" => Ok(AsvoJobType::Imaging),
            _ => Ok(AsvoJobType::Unknown),
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
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub struct AsvoFilesArray {
    #[serde(rename = "jobType")]
    pub r#type: Delivery,
    #[serde(rename = "fileUrl")]
    pub url: Option<String>,
    #[serde(rename = "filePath")]
    pub path: Option<String>,
    #[serde(rename = "fileSize")]
    pub size: u64,
    #[serde(rename = "fileHash")]
    pub sha1: Option<String>,
}

/// A simple type alias. Not using a newtype, because that would produce
/// unnecessary complexity.
pub type AsvoJobID = u32;

/// All of the metadata associated with an ASVO job.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub struct AsvoJob {
    pub obsid: Obsid,
    #[serde(rename = "jobId")]
    pub jobid: AsvoJobID,
    #[serde(rename = "jobType")]
    pub jtype: AsvoJobType,
    #[serde(rename = "jobState")]
    pub state: AsvoJobState,
    pub files: Option<Vec<AsvoFilesArray>>,
    pub completed: Option<DateTime<Utc>>,
}

/// A vector of ASVO jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Clone)]
pub struct AsvoJobVec(pub Vec<AsvoJob>);

impl AsvoJobVec {
    /// Get a vector of ASVO jobs in JSON form.
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
pub struct AsvoJobMap(pub BTreeMap<AsvoJobID, AsvoJob>);

impl From<AsvoJobVec> for AsvoJobMap {
    fn from(job_vec: AsvoJobVec) -> AsvoJobMap {
        let mut tree = BTreeMap::new();
        for j in job_vec.0.into_iter() {
            tree.insert(j.jobid, j);
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
            "Job ID: {jobid}, obsid: {obsid}, type: {type}, state: {state}, product_array: {files:?}",
            obsid=self.obsid,
            jobid=self.jobid,
            type=self.jtype,
            state=self.state,
            files=self.files,
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
        jobid: AsvoJobID,
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
}
