// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/*!
 * ASVO data types.
*/

use std::collections::BTreeMap;

use prettytable::{cell, row, Cell, Row, Table};
use serde::Serialize;

use crate::obsid::Obsid;

/// All of the available types of ASVO jobs.
#[derive(Serialize, PartialEq, Debug, Clone)]
pub enum AsvoJobType {
    Conversion,
    DownloadVisibilities,
    DownloadMetadata,
    DownloadVoltage,
    CancelJob,
}

/// All of states an ASVO job may be in.
#[derive(Serialize, PartialEq, Debug, Clone)]
pub enum AsvoJobState {
    Queued,
    Processing,
    Ready,
    Error(String),
    Expired,
    Cancelled,
}

/// A single file provided by an ASVO job.
#[derive(Serialize, PartialEq, Debug)]
pub struct AsvoFilesArray {
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
    #[serde(rename = "fileHash")]
    pub sha1: String,
}

/// A simple type alias. Not using a newtype, because that would produce
/// unnecessary complexity.
pub type AsvoJobID = u32;

/// All of the metadata associated with an ASVO job.
#[derive(Serialize, PartialEq, Debug)]
pub struct AsvoJob {
    pub obsid: Obsid,
    #[serde(rename = "jobId")]
    pub jobid: AsvoJobID,
    #[serde(rename = "jobType")]
    pub jtype: AsvoJobType,
    #[serde(rename = "jobState")]
    pub state: AsvoJobState,
    pub files: Option<Vec<AsvoFilesArray>>,
}

/// A vector of ASVO jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
pub struct AsvoJobVec(pub Vec<AsvoJob>);

impl AsvoJobVec {
    /// Render a slice of `AsvoJob` in a pretty-printed table.
    pub fn list(self) {
        if self.0.is_empty() {
            println!("You have no jobs.");
        } else {
            let mut table = Table::new();
            table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
            table.set_titles(row![
                b => "Job ID",
                "Obsid",
                "Job Type",
                "Job State",
                "File Size"
            ]);
            for j in self.0 {
                table.add_row(Row::new(vec![
                    Cell::new(j.jobid.to_string().as_str()),
                    Cell::new(j.obsid.to_string().as_str()),
                    Cell::new(j.jtype.to_string().as_str()).style_spec(match j.jtype {
                        AsvoJobType::Conversion => "Fb",
                        AsvoJobType::DownloadVisibilities => "Fb",
                        AsvoJobType::DownloadMetadata => "Fy",
                        AsvoJobType::DownloadVoltage => "Fm",
                        AsvoJobType::CancelJob => "Fr",
                    }),
                    Cell::new(j.state.to_string().as_str()).style_spec(match j.state {
                        AsvoJobState::Queued => "Fm",
                        AsvoJobState::Processing => "Fb",
                        AsvoJobState::Ready => "Fg",
                        AsvoJobState::Error(_) => "Fr",
                        AsvoJobState::Expired => "Fr",
                        AsvoJobState::Cancelled => "Fr",
                    }),
                    Cell::new(
                        match j.files {
                            None => "".to_string(),
                            Some(v) => {
                                let mut size = 0;
                                for f in v {
                                    size += f.file_size;
                                }
                                bytesize::ByteSize(size).to_string_as(true)
                            }
                        }
                        .as_str(),
                    ),
                ]));
            }
            table.printstd();
        }
    }

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
}

/// A `BTreeMap` of ASVO job IDs against their jobs. Useful for efficiently
/// isolating specific jobs.
///
/// By using a custom type, custom methods can be easily defined and used.
#[derive(Serialize, PartialEq, Debug)]
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
                Self::Conversion => "Conversion",
                Self::DownloadVisibilities => "Download Visibilities",
                Self::DownloadMetadata => "Download Metadata",
                Self::DownloadVoltage => "Download Voltage",
                Self::CancelJob => "Cancel Job",
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
                Self::Queued => "Queued".to_string(),
                Self::Processing => "Processing".to_string(),
                Self::Ready => "Ready".to_string(),
                Self::Error(e) => format!("Error: {}", e),
                Self::Expired => "Expired".to_string(),
                Self::Cancelled => "Cancelled".to_string(),
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
