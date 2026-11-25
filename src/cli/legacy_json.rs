// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The old `--json` format of `giant-squid list` and `wait`, for
//! `--legacy-json`.
//!
//! Before 3.0.0, the job JSON used its own camelCase keys (`obsid`,
//! `jobId`, `jobType`, `jobState`, and `fileUrl`, `filePath`, `fileSize`,
//! `fileHash` for each file). It also put a file's delivery type under
//! `jobType`, and it had no times. From 3.0.0, `--json` prints the OpenAPI
//! names (see [`MwaAsvoJobVec::json`]). This module keeps the format of
//! giant-squid 2.5.1, byte for byte, so that scripts have time to move over. It is only in the CLI:
//! the library and the Python module have only the new format.
//!
//! Remove this module, and `--legacy-json`, in a later release (when the
//! users no longer need it).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::mwa_asvo::api::openapi::DeliveryDestination;
use crate::mwa_asvo::{JobFile, JobState, JobType, MwaAsvoJob, MwaAsvoJobId, MwaAsvoJobVec};
use crate::obs_id::ObsId;

/// The warning logged (to stderr, like every log record) when
/// `--legacy-json` is used.
pub const LEGACY_JSON_WARNING: &str = "--legacy-json is deprecated and will be removed in a \
     later release. Use --json, which prints the OpenAPI names (obs_id, id, job_type, job_state; \
     and type, url, path, size, sha1 for each file).";

/// One file, with the old keys. The field order is the old key order.
#[derive(Serialize)]
struct LegacyFile<'a> {
    #[serde(rename = "jobType")]
    delivery: &'static str,
    #[serde(rename = "fileUrl")]
    url: &'a Option<String>,
    #[serde(rename = "filePath")]
    path: &'a Option<String>,
    #[serde(rename = "fileSize")]
    size: u64,
    #[serde(rename = "fileHash")]
    sha1: &'a Option<String>,
}

/// A job state, as the old format wrote it: a name in CamelCase, `Ready`
/// for a completed job, and `{"Error": "<message>"}` for a job with an
/// error.
#[derive(Serialize)]
enum LegacyState<'a> {
    Queued,
    WaitCal,
    Staging,
    Staged,
    Preparing,
    Downloading,
    Preprocessing,
    Imaging,
    Delivering,
    Ready,
    Error(&'a str),
    Cancelled,
}

impl<'a> LegacyState<'a> {
    /// The old form of `job`'s state.
    fn of(job: &'a MwaAsvoJob) -> Self {
        match job.job_state {
            JobState::Queued => Self::Queued,
            JobState::Waitcal => Self::WaitCal,
            JobState::Staging => Self::Staging,
            JobState::Staged => Self::Staged,
            JobState::Preparing => Self::Preparing,
            JobState::Downloading => Self::Downloading,
            JobState::Preprocessing => Self::Preprocessing,
            JobState::Imaging => Self::Imaging,
            JobState::Delivering => Self::Delivering,
            JobState::Completed => Self::Ready,
            JobState::Error => Self::Error(job.error_text.as_deref().unwrap_or_default()),
            JobState::Cancelled => Self::Cancelled,
        }
    }
}

/// A job type, as the old format wrote it (for example
/// `DownloadVisibilities`).
fn legacy_job_type(job_type: JobType) -> &'static str {
    match job_type {
        JobType::Conversion => "Conversion",
        JobType::Visibility => "DownloadVisibilities",
        JobType::Metadata => "DownloadMetadata",
        JobType::Voltage => "DownloadVoltage",
        JobType::Cancel => "CancelJob",
        JobType::Beamformer => "DownloadBeamformer",
        JobType::Imaging => "Imaging",
    }
}

/// One job, with the old keys. The field order is the old key order.
#[derive(Serialize)]
struct LegacyJob<'a> {
    obsid: ObsId,
    #[serde(rename = "jobId")]
    job_id: MwaAsvoJobId,
    #[serde(rename = "jobType")]
    job_type: &'static str,
    #[serde(rename = "jobState")]
    job_state: LegacyState<'a>,
    files: Option<Vec<LegacyFile<'a>>>,
}

impl<'a> From<&'a JobFile> for LegacyFile<'a> {
    fn from(file: &'a JobFile) -> Self {
        Self {
            // The old format wrote the delivery in CamelCase.
            delivery: match file.type_ {
                DeliveryDestination::Acacia => "Acacia",
                DeliveryDestination::Dug => "Dug",
                DeliveryDestination::Scratch => "Scratch",
            },
            url: &file.url,
            path: &file.path,
            size: file.size_bytes(),
            sha1: &file.sha1,
        }
    }
}

impl<'a> From<&'a MwaAsvoJob> for LegacyJob<'a> {
    fn from(job: &'a MwaAsvoJob) -> Self {
        Self {
            obsid: job.obs_id(),
            job_id: job.job_id(),
            job_type: legacy_job_type(job.job_type),
            job_state: LegacyState::of(job),
            // The old format had the file list at the top level.
            files: job
                .product
                .as_ref()
                .map(|product| product.files.iter().map(LegacyFile::from).collect()),
        }
    }
}

/// The jobs in the old JSON format: an object keyed by Job ID, in Job ID
/// order, as [`MwaAsvoJobVec::json`] is. As there, a Job ID that is listed
/// more than once appears once.
///
/// # Errors
///
/// A `serde_json` error, which does not happen for these types in
/// practice.
pub fn to_legacy_json(jobs: &MwaAsvoJobVec) -> Result<String, serde_json::Error> {
    let map: BTreeMap<MwaAsvoJobId, LegacyJob> = jobs
        .0
        .iter()
        .map(|job| (job.job_id(), LegacyJob::from(job)))
        .collect();
    serde_json::to_string(&map)
}
