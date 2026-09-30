// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The old `--json` format of `giant-squid list` and `wait`, for
//! `--legacy-json`.
//!
//! Before 3.0.0, the job JSON used its own camelCase keys (`obsid`,
//! `jobId`, `jobType`, `jobState`, and `fileUrl`, `filePath`, `fileSize`,
//! `fileHash` for each file). It also put a file's delivery type under
//! `jobType`. From 3.0.0, `--json` prints the OpenAPI names (see
//! [`AsvoJobVec::json`]). This module keeps the old format, byte for byte,
//! so that scripts have one release to move over. It is only in the CLI:
//! the library and the Python module have only the new format.
//!
//! Remove this module, and `--legacy-json`, in the release after 3.0.0.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::Serialize;

use crate::asvo::{
    AsvoFilesArray, AsvoJob, AsvoJobId, AsvoJobState, AsvoJobType, AsvoJobVec, Delivery,
};
use crate::obs_id::ObsId;

/// The warning logged (to stderr, like every log record) when
/// `--legacy-json` is used.
pub const LEGACY_JSON_WARNING: &str = "--legacy-json is deprecated and will be removed in the \
     release after 3.0.0. Use --json, which prints the OpenAPI names (obs_id, job_id, job_type, \
     job_state; and type, url, path, size, sha1 for each file).";

/// One file, with the old keys. The field order is the old key order.
#[derive(Serialize)]
struct LegacyFile<'a> {
    #[serde(rename = "jobType")]
    delivery: &'a Delivery,
    #[serde(rename = "fileUrl")]
    url: &'a Option<String>,
    #[serde(rename = "filePath")]
    path: &'a Option<String>,
    #[serde(rename = "fileSize")]
    size: u64,
    #[serde(rename = "fileHash")]
    sha1: &'a Option<String>,
}

/// One job, with the old keys. The field order is the old key order.
#[derive(Serialize)]
struct LegacyJob<'a> {
    obsid: &'a ObsId,
    #[serde(rename = "jobId")]
    job_id: AsvoJobId,
    #[serde(rename = "jobType")]
    job_type: &'a AsvoJobType,
    #[serde(rename = "jobState")]
    job_state: &'a AsvoJobState,
    files: Option<Vec<LegacyFile<'a>>>,
    completed: &'a Option<Timestamp>,
}

impl<'a> From<&'a AsvoFilesArray> for LegacyFile<'a> {
    fn from(file: &'a AsvoFilesArray) -> Self {
        Self {
            delivery: &file.r#type,
            url: &file.url,
            path: &file.path,
            size: file.size,
            sha1: &file.sha1,
        }
    }
}

impl<'a> From<&'a AsvoJob> for LegacyJob<'a> {
    fn from(job: &'a AsvoJob) -> Self {
        Self {
            obsid: &job.obs_id,
            job_id: job.job_id,
            job_type: &job.job_type,
            job_state: &job.job_state,
            // The old format had the file list at the top level.
            files: job
                .product
                .as_ref()
                .map(|product| product.files.iter().map(LegacyFile::from).collect()),
            completed: &job.completed,
        }
    }
}

/// The jobs in the old JSON format: an object keyed by job ID, in job ID
/// order, as [`AsvoJobVec::json`] is. As there, a job ID that is listed
/// more than once appears once.
///
/// # Errors
///
/// A `serde_json` error, which does not happen for these types in
/// practice.
pub fn to_legacy_json(jobs: &AsvoJobVec) -> Result<String, serde_json::Error> {
    let map: BTreeMap<AsvoJobId, LegacyJob> = jobs
        .0
        .iter()
        .map(|job| (job.job_id, LegacyJob::from(job)))
        .collect();
    serde_json::to_string(&map)
}
