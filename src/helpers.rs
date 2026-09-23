// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small helper utility functions.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::{fs, io};

use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::asvo::*;
use crate::obsid::Obsid;

enum ObsidOrJobID {
    /// This is an obsid.
    O(Obsid),
    /// This is a job ID.
    J(AsvoJobID),
}

fn parse_jobid_or_obsid(s: &str) -> Option<ObsidOrJobID> {
    match s.parse::<u64>() {
        // We successfully parsed an int.
        Ok(i) => {
            match Obsid::validate(i) {
                // This int is an obsid.
                Ok(o) => Some(ObsidOrJobID::O(o)),
                // This int isn't an obsid; assume it is a jobid.
                Err(_) => Some(ObsidOrJobID::J(i as AsvoJobID)),
            }
        }
        // Could not parse the string as an int; we must fail.
        Err(_) => None,
    }
}

/// Read a file, and return two vectors of ASVO job IDs and obsids. Fail if any
/// string in the file cannot be parsed as either.
pub fn parse_jobids_and_obsids_from_file<T: AsRef<Path>>(
    f: T,
) -> Result<(Vec<AsvoJobID>, Vec<Obsid>), ParseError> {
    let mut obsids = vec![];
    let mut jobids = vec![];

    // Open the file.
    let mut reader = std::io::BufReader::new(std::fs::File::open(&f)?);
    let mut line = String::new();
    // For each line...
    while reader.read_line(&mut line)? > 0 {
        // ... split the whitespace and try to parse
        // obsids. Fail if whitespace-delimited text
        // can't be parsed into an int.
        for text in line.split_whitespace() {
            match parse_jobid_or_obsid(text) {
                Some(ObsidOrJobID::O(obsid)) => obsids.push(obsid),
                Some(ObsidOrJobID::J(jobid)) => jobids.push(jobid),
                // `text` could not be parsed; so we must fail.
                None => {
                    return Err(ParseError::InsideFile {
                        file: f.as_ref().display().to_string(),
                        text: text.to_string(),
                    })
                }
            }
        }
        line.clear();
    }

    Ok((jobids, obsids))
}

/// Parse a string of ASVO job IDs, obsids, or files containing job IDs or
/// obsids into two vectors of job IDs and obsids.
pub fn parse_many_jobids_or_obsids(
    strings: &[String],
) -> Result<(Vec<AsvoJobID>, Vec<Obsid>), ParseError> {
    // Attempt to parse all arguments as ints. If they aren't 10
    // digits long, assume they are ASVO job IDs. If any argument is
    // not an int, assume it is a file. Exit on any error.
    let mut jobids = vec![];
    let mut obsids = vec![];
    for s in strings {
        match parse_jobid_or_obsid(s) {
            Some(ObsidOrJobID::O(obsid)) => obsids.push(obsid),
            Some(ObsidOrJobID::J(jobid)) => jobids.push(jobid),
            // Could not parse the string as an int; assume it is a
            // file and unpack it.
            None => {
                let (mut j, mut o) = parse_jobids_and_obsids_from_file(s)?;
                jobids.append(&mut j);
                obsids.append(&mut o);
            }
        }
    }

    Ok((jobids, obsids))
}

#[derive(Error, Debug)]
pub enum ParseError {
    /// When a whitespace-delimited string inside a file isn't an integer, this
    /// error can be used.
    #[error("'{text}' in file {file} could not be parsed as an int.")]
    InsideFile { file: String, text: String },

    /// An IO error.
    #[error("{0}")]
    IO(#[from] std::io::Error),
}

/// Takes a filename, expected hash and a job id and returns
/// Ok if the calculated hash matches the expected hash, otherwise
/// returns an AsvoError::HashMismatch
pub fn check_file_sha1_hash(
    filename: &PathBuf,
    expected_hash: &str,
    job_id: u32,
) -> Result<(), AsvoError> {
    let mut file = fs::File::open(filename)?;
    let mut hasher = Sha1::new();
    io::copy(&mut file, &mut hasher)?;
    let hash = format!("{:x}", hasher.finalize());

    if hash.eq_ignore_ascii_case(expected_hash) {
        Ok(())
    } else {
        Err(AsvoError::HashMismatch {
            jobid: job_id,
            file: filename.display().to_string(),
            calculated_hash: hash,
            expected_hash: expected_hash.to_string(),
        })
    }
}

pub fn get_job_type_table_style(job_type: AsvoJobType, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_type {
            AsvoJobType::Conversion => "Fb",
            AsvoJobType::DownloadVisibilities => "Fb",
            AsvoJobType::DownloadBeamformer => "Fb",
            AsvoJobType::DownloadMetadata => "Fy",
            AsvoJobType::DownloadVoltage => "Fm",
            AsvoJobType::CancelJob => "Fr",
            AsvoJobType::Imaging => "Fb",
            AsvoJobType::Unknown => "Fr",
        }
        .to_string()
    }
}

pub fn get_job_state_table_style(job_state: AsvoJobState, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_state {
            AsvoJobState::Queued => "FW",
            AsvoJobState::WaitCal => "Fm",
            AsvoJobState::Staging => "Fm",
            AsvoJobState::Staged => "Fm",
            AsvoJobState::Preparing => "Fm",
            AsvoJobState::Downloading => "Fm",
            AsvoJobState::Preprocessing => "Fm",
            AsvoJobState::Imaging => "Fm",
            AsvoJobState::Delivering => "Fm",
            AsvoJobState::Ready => "Fg",
            AsvoJobState::Error(_) => "Fr",
            AsvoJobState::Expired => "Fw",
            AsvoJobState::Cancelled => "Fr",
        }
        .to_string()
    }
}

#[cfg(test)]
mod test;
