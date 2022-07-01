// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to parse the insane json format returned by the ASVO.

use std::collections::HashMap;

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
    priority: i8,
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
                    r#type: dumb_product.r#type.clone(),
                    url: dumb_product.url.clone(),
                    path: None,
                    size: dumb_product.size,
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
    JobID { job_id: AsvoJobID },
    ErrorWithCode { error_code: u32, error: String },
    GenericError { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_job_listing_parse() {
        let json = "[{\"action\": \"INSERT\", \"table\": \"jobs\", \"row\": {\"job_type\": 1, \"job_state\": 2, \"user_id\": 1065, \"job_params\": {\"delivery\": \"acacia\", \"download_type\": \"vis\", \"job_type\": \"download\", \"obs_id\": \"1339896408\", \"priority\": 1, \"user_pawsey_group\": \"mwaops\"}, \"error_code\": null, \"error_text\": null, \"created\": \"2022-06-22T01:56:38.635146\", \"started\": \"2022-06-22T01:57:09.093927\", \"completed\": \"2022-06-22T01:57:24.693448\", \"product\": {\"files\": [{\"type\": \"acacia\", \"url\": \"https://ingest.pawsey.org.au/mwa-asvo/1339896408_575929_vis.tar?AWSAccessKeyId=0f61c75cd1184e5abc76500d71758927&Signature=XwoaCna8vNmMEBXcFji2boZ5yjk%3D&Expires=1656467844\", \"size\": 931112960, \"sha1\": \"12b0933ff3985c82a7303d8e57fa7157fe88353e\"}]}, \"id\": 575929}}]";
        let result = parse_asvo_json(json);
        assert!(
            result.is_ok(),
            "result is not ok: {:?}",
            result.err().unwrap()
        );
        let jobs = result.unwrap();
        assert_eq!(jobs.0.len(), 1);
        assert_eq!(jobs.0[0].jobid, 575929);
    }

    #[test]
    fn test_json_job_submit_response_parse() {
        let json = "{\"job_id\": 308874}";
        let decoded = serde_json::from_str::<AsvoSubmitJobResponse>(json);
        assert!(decoded.is_ok());
        assert_eq!(
            AsvoSubmitJobResponse::JobID { job_id: 308874 },
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
