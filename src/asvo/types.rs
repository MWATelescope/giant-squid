// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ASVO data types.

use std::{collections::BTreeMap, str::FromStr};

use log::warn;
use prettytable::{row, Cell, Row, Table};
use serde::Serialize;

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
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub enum AsvoJobType {
    Conversion,
    DownloadVisibilities,
    DownloadMetadata,
    DownloadVoltage,
    CancelJob,
}

impl FromStr for AsvoJobType {
    type Err = AsvoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match _sanitize_identifier(s).as_str() {
            "conversion" => Ok(AsvoJobType::Conversion),
            "downloadvisibilities" => Ok(AsvoJobType::DownloadVisibilities),
            "downloadmetadata" => Ok(AsvoJobType::DownloadMetadata),
            "downloadvoltage" => Ok(AsvoJobType::DownloadVoltage),
            "canceljob" => Ok(AsvoJobType::CancelJob),
            _ => Err(AsvoError::InvalidJobType { str: s.to_string() }),
        }
    }
}

/// All of states an ASVO job may be in.
#[derive(Serialize, PartialEq, Eq, Debug, Clone)]
pub enum AsvoJobState {
    Queued,
    Processing,
    Ready,
    Error(String),
    Expired,
    Cancelled,
}

impl FromStr for AsvoJobState {
    type Err = AsvoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match _sanitize_identifier(s).as_str() {
            "queued" => Ok(AsvoJobState::Queued),
            "processing" => Ok(AsvoJobState::Processing),
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
#[derive(Serialize, PartialEq, Eq, Debug)]
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
                "File Size",
                "Delivery"
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
                        match &j.files {
                            None => "".to_string(),
                            Some(v) => {
                                let mut size = 0;
                                for f in v {
                                    size += f.size;
                                }
                                bytesize::ByteSize(size).to_string_as(true)
                            }
                        }
                        .as_str(),
                    ),
                    Cell::new(
                        match j.files {
                            None => "".to_string(),
                            Some(v) => v.first().unwrap().r#type.to_string(),
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
                AsvoJobType::CancelJob => "Cancel Job",
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
                AsvoJobState::Processing => "Processing".to_string(),
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

    /// Deliver the ASVO job to the /scratch filesystem at the Pawsey
    /// Supercomputing Centre.
    Scratch,
}

impl Delivery {
    pub fn validate<S: AsRef<str>>(d: Option<S>) -> Result<Delivery, AsvoError> {
        match (d, std::env::var("GIANT_SQUID_DELIVERY")) {
            (Some(d), _) => match d.as_ref() {
                "acacia" => Ok(Delivery::Acacia),
                "scratch" => Ok(Delivery::Scratch),
                d => Err(AsvoError::InvalidDelivery(d.to_string())),
            },
            (None, Ok(d)) => match d.as_str() {
                "acacia" => Ok(Delivery::Acacia),
                "scratch" => Ok(Delivery::Scratch),
                d => Err(AsvoError::InvalidDeliveryEnv(d.to_string())),
            },
            (None, Err(std::env::VarError::NotPresent)) => {
                warn!("Using 'acacia' for ASVO delivery");
                Ok(Delivery::Acacia)
            }
            (None, Err(std::env::VarError::NotUnicode(_))) => {
                Err(AsvoError::InvalidDeliveryEnvUnicode)
            }
        }
    }
}

impl std::fmt::Display for Delivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Delivery::Acacia => "acacia",
                Delivery::Scratch => "scratch",
            }
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum DeliveryFormat {
    /// Tar - tar up all files. Only relevant for non-voltage scratch jobs    
    Tar,
}

impl DeliveryFormat {
    pub fn validate<S: AsRef<str>>(d: Option<S>) -> Result<Option<DeliveryFormat>, AsvoError> {
        match (d, std::env::var("GIANT_SQUID_DELIVERY_FORMAT")) {
            (Some(d), _) => match d.as_ref() {
                "tar" => Ok(Some(DeliveryFormat::Tar)),
                d => Err(AsvoError::InvalidDeliveryFormat(d.to_string())),
            },
            (None, Ok(d)) => match d.as_str() {
                "tar" => Ok(Some(DeliveryFormat::Tar)),
                d => Err(AsvoError::InvalidDeliveryFormatEnv(d.to_string())),
            },
            (None, Err(std::env::VarError::NotPresent)) => Ok(None),
            (None, Err(std::env::VarError::NotUnicode(_))) => {
                Err(AsvoError::InvalidDeliveryFormatEnvUnicode)
            }
        }
    }
}

impl std::fmt::Display for DeliveryFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DeliveryFormat::Tar => "tar",
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asvo_job_state_fromstr() {
        assert!(matches!(
            AsvoJobState::from_str("__Q_u_e_u_e_D__"),
            Ok(AsvoJobState::Queued)
        ));
        assert!(matches!(
            AsvoJobState::from_str("invalid job state"),
            Err(AsvoError::InvalidJobState { .. })
        ));
    }

    #[test]
    fn test_asvo_job_type_fromstr() {
        assert!(matches!(
            AsvoJobType::from_str("DownloadVisibilities"),
            Ok(AsvoJobType::DownloadVisibilities)
        ));
        assert!(matches!(
            AsvoJobType::from_str("download_visibilities"),
            Ok(AsvoJobType::DownloadVisibilities)
        ));
        assert!(matches!(
            AsvoJobType::from_str("invalid job type"),
            Err(AsvoError::InvalidJobType { .. })
        ));
    }
}
