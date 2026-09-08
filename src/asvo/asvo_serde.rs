// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to parse the insane json format returned by the ASVO.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use super::types::*;
use crate::obsid::Obsid;

pub(super) fn parse_asvo_json(json: &str) -> Result<AsvoJobVec, serde_json::error::Error> {
    let strings: Vec<DummyJob> = serde_json::from_str(json)?;
    let vec = strings
        .into_iter()
        .map(|dj| dj.convert_to_real_job())
        .collect::<Vec<AsvoJob>>();
    Ok(AsvoJobVec(vec))
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct DummyJobParams {
    delivery: String,
    download_type: Option<String>,
    obs_id: String, // The JSON decoding requires this to be a string, but it should always be a 10-digit int.
    job_type: String,
    priority: u16,
    user_pawsey_group: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct DummyProduct {
    r#type: String,
    url: Option<String>,
    path: Option<String>,
    size: u64,
    sha1: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DummyRow {
    job_type: u8,
    id: AsvoJobID,
    job_state: String,
    job_params: DummyJobParams,
    error_text: Option<String>,
    product: Option<HashMap<String, Vec<DummyProduct>>>,
    completed: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DummyJob {
    row: DummyRow,
}

impl DummyJob {
    fn convert_to_real_job(self) -> AsvoJob {
        let new_files = self.row.product.map(|hm| {
            let mut file_array = vec![];
            for dumb_product in &hm["files"] {
                let file_type = dumb_product.r#type.as_str();
                file_array.push(AsvoFilesArray {
                    r#type: match file_type {
                        "acacia" => Delivery::Acacia,
                        "dug" => Delivery::Dug,
                        "scratch" => Delivery::Scratch,
                        _ => panic!("Unsupported delivery type found: {}", file_type),
                    },
                    url: dumb_product.url.clone(),
                    path: dumb_product.r#path.clone(),
                    size: dumb_product.size,
                    sha1: dumb_product.sha1.clone(),
                })
            }
            file_array
        });

        println!("{:?}", self.row.completed);

        // if there is a "completed" value, convert to a date/time
        let completed_dt: Option<DateTime<Utc>> = self
            .row
            .completed
            .as_deref()
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok())
            .map(|dt| dt.and_utc());

        AsvoJob {
            obsid: Obsid::validate(self.row.job_params.obs_id.parse().unwrap()).unwrap(),
            jobid: self.row.id,
            jtype: match self.row.job_type {
                0 => AsvoJobType::Conversion,
                1 => AsvoJobType::DownloadVisibilities,
                2 => AsvoJobType::DownloadMetadata,
                3 => AsvoJobType::DownloadVoltage,
                4 => AsvoJobType::CancelJob,
                5 => AsvoJobType::DownloadBeamformer,
                6 => AsvoJobType::Imaging,
                _ => AsvoJobType::Unknown,
            },
            state: match self.row.job_state.as_str() {
                "queued" => AsvoJobState::Queued,
                "waitcal" => AsvoJobState::WaitCal,
                "staging" => AsvoJobState::Staging,
                "staged" => AsvoJobState::Staged,
                "downloading" => AsvoJobState::Downloading,
                "preprocessing" => AsvoJobState::Preprocessing,
                "preparing" => AsvoJobState::Preparing,
                "imaging" => AsvoJobState::Imaging,
                "delivering" => AsvoJobState::Delivering,
                "completed" => AsvoJobState::Ready,
                "error" => AsvoJobState::Error(self.row.error_text.unwrap()),
                "expired" => AsvoJobState::Expired,
                "cancelled" => AsvoJobState::Cancelled,
                _ => panic!("Unrecognised job_state! {}", self.row.job_state.as_str()),
            },
            files: new_files,
            completed: completed_dt,
        }
    }
}

/// When defining serde structs remember order matters!
/// Put the most specific matches first, then less
/// specific last!
#[derive(Deserialize, PartialEq, Debug)]
#[serde(untagged)]
pub(super) enum AsvoSubmitJobResponse {
    JobIDWithError {
        error: String,
        error_code: u32,
        job_id: AsvoJobID,
    },
    JobID {
        job_id: AsvoJobID,
    },
    ErrorWithCode {
        error_code: u32,
        error: String,
    },
    GenericError {
        error: String,
    },
}
