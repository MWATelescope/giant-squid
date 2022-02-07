// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to parse the insane json format returned by the ASVO.

use std::collections::HashMap;

use serde::Deserialize;

use super::types::*;
use crate::obsid::Obsid;

pub(super) fn parse_asvo_json(json: &str) -> Result<AsvoJobVec, serde_json::error::Error> {
    // For some reason, the jobs are stored as strings.
    let strings: Vec<String> = serde_json::from_str(json)?;
    let vec = strings
        .into_iter()
        .map(|s| {
            let dj: Result<DummyJob, _> = serde_json::from_str(&s);
            dj.map(|j| j.convert_to_real_job())
        })
        .collect::<Result<Vec<AsvoJob>, _>>();
    vec.map(AsvoJobVec)
}

#[derive(Deserialize, Debug)]
struct DummyJobParams {
    _download_type: Option<String>,
    // The JSON decoding requires this to be a string, but it should always be a
    // 10-digit int.
    obs_id: String,
}

#[derive(Deserialize, Debug)]
struct DummyProduct {
    file_name: String,
    file_size: u64,
    sha1: String,
}

#[derive(Deserialize, Debug)]
struct DummyRow {
    job_type: u8,
    id: AsvoJobID,
    job_state: u8,
    job_params: DummyJobParams,
    error_text: Option<String>,
    product: Option<HashMap<String, Vec<DummyProduct>>>,
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
                file_array.push(AsvoFilesArray {
                    file_name: dumb_product.file_name.clone(),
                    file_size: dumb_product.file_size,
                    sha1: dumb_product.sha1.clone(),
                })
            }
            file_array
        });
        AsvoJob {
            obsid: Obsid::validate(self.row.job_params.obs_id.parse().unwrap()).unwrap(),
            jobid: self.row.id,
            jtype: match self.row.job_type {
                0 => AsvoJobType::Conversion,
                1 => AsvoJobType::DownloadVisibilities,
                2 => AsvoJobType::DownloadMetadata,
                3 => AsvoJobType::DownloadVoltage,
                4 => AsvoJobType::CancelJob,
                _ => panic!("Unrecognised job_type!"),
            },
            state: match self.row.job_state {
                0 => AsvoJobState::Queued,
                1 => AsvoJobState::Processing,
                2 => AsvoJobState::Ready,
                3 => AsvoJobState::Error(self.row.error_text.unwrap()),
                4 => AsvoJobState::Expired,
                5 => AsvoJobState::Cancelled,
                _ => panic!("Unrecognised job_state!"),
            },
            files: new_files,
        }
    }
}

#[derive(Deserialize, PartialEq, Debug)]
#[serde(untagged)]
pub(super) enum AsvoSubmitJobResponse {
    JobID { job_id: AsvoJobID, new: bool },
    ErrorWithCode { error_code: u32, error: String },
    GenericError { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_job_listing_parse() {
        let json = "[\"{\\\"action\\\": \\\"INSERT\\\", \\\"table\\\": \\\"jobs\\\", \\\"row\\\": {\\\"job_type\\\": 1, \\\"job_state\\\": 2, \\\"user_id\\\": 92, \\\"job_params\\\": {\\\"download_type\\\": \\\"vis\\\", \\\"obs_id\\\": \\\"1065880128\\\"}, \\\"error_code\\\": null, \\\"error_text\\\": null, \\\"created\\\": \\\"2020-08-20T04:17:24.075207\\\", \\\"modified\\\": \\\"2020-08-20T04:29:40.020931\\\", \\\"expiry_date\\\": \\\"2020-08-27T04:29:39.822127\\\", \\\"product\\\": {\\\"files\\\": [[\\\"1065880128_vis.zip\\\", 44658597858, \\\"f561aa665fd6367c05a89f7e2931b60c289348de\\\"]]}, \\\"storage_id\\\": 3, \\\"id\\\": 306792}}\"]";
        let result = parse_asvo_json(json);
        assert!(result.is_ok());
        let jobs = result.unwrap();
        assert_eq!(jobs.0.len(), 1);
        assert_eq!(jobs.0[0].jobid, 306792);
    }

    #[test]
    fn test_json_job_submit_response_parse() {
        let json = "{\"job_id\": 308874, \"new\": false}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::JobID {
                job_id: 308874,
                new: false
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_bad_parse() {
        let json = "{\"error_code\": 0, \"error\": \"Download Type: Expected not None\"}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::ErrorWithCode {
                error_code: 0,
                error: "Download Type: Expected not None".to_string(),
            },
            decoded.unwrap()
        );
    }

    #[test]
    fn test_json_job_submit_response_bad_parse2() {
        let json = "{\"error\": \"Permission denied\"}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::GenericError {
                error: "Permission denied".to_string(),
            },
            decoded.unwrap()
        );
    }
}
