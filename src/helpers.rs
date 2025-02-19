// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small helper utility functions.

use std::collections::BTreeMap;
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

/// Parse a string of key-value pairs (e.g. "avg_time_res=0.5,avg_freq_res=10") into a
/// [BTreeMap].
pub fn parse_key_value_pairs(s: &str) -> Result<BTreeMap<&str, &str>, ParseError> {
    let mut map = BTreeMap::new();
    for pair in s.split(',') {
        let mut key = "";
        let mut value = "";
        let mut items = 0;
        for item in pair.split('=') {
            match items {
                0 => {
                    key = item.trim();
                    items += 1;
                }
                1 => {
                    value = item.trim();
                    items += 1;
                }
                _ => {
                    return Err(ParseError::NotKeyValue(pair.to_string()));
                }
            }
        }
        if items != 2 {
            return Err(ParseError::NotKeyValue(pair.to_string()));
        }

        map.insert(key, value);
    }
    Ok(map)
}

#[derive(Error, Debug)]
pub enum ParseError {
    /// When a whitespace-delimited string inside a file isn't an integer, this
    /// error can be used.
    #[error("'{text}' in file {file} could not be parsed as an int.")]
    InsideFile { file: String, text: String },

    /// Invalid number of items when parsing key-value pairs.
    #[error("Could not parse {0} into a key-value pair.")]
    NotKeyValue(String),

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
            AsvoJobType::DownloadMetadata => "Fy",
            AsvoJobType::DownloadVoltage => "Fm",
            AsvoJobType::CancelJob => "Fr",
        }
        .to_string()
    }
}

pub fn get_job_state_table_style(job_state: AsvoJobState, no_colour: bool) -> String {
    if no_colour {
        "".to_string()
    } else {
        match job_state {
            AsvoJobState::Queued => "Fw",
            AsvoJobState::WaitCal => "Fm",
            AsvoJobState::Staging => "Fm",
            AsvoJobState::Staged => "Fm",
            AsvoJobState::Downloading => "Fm",
            AsvoJobState::Preprocessing => "Fm",
            AsvoJobState::Imaging => "Fm",
            AsvoJobState::Delivering => "Fm",
            AsvoJobState::Ready => "Fg",
            AsvoJobState::Error(_) => "Fr",
            AsvoJobState::Expired => "Fr",
            AsvoJobState::Cancelled => "Fr",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn check_file_sha1_hash_ok() {
        // Create test file of known sha1sum hash
        let mut tmpfile = NamedTempFile::new().expect("Could not create tmp file");
        write!(tmpfile, "Hello World!").unwrap();
        tmpfile.flush().expect("Error flushing tmp file");

        // Check the checksum of the tmp file
        assert!(check_file_sha1_hash(
            &tmpfile.path().to_path_buf(),
            "2ef7bde608ce5404e97d5f042f95f89f1c232871",
            123
        )
        .is_ok());
    }

    #[test]
    fn check_file_sha1_hash_err() {
        // Create test file of known sha1sum hash
        let mut tmpfile = NamedTempFile::new().expect("Could not create tmp file");
        write!(tmpfile, "Hello World!").unwrap();
        tmpfile.flush().expect("Error flushing tmp file");

        // Check the checksum of the tmp file - but the expected checksum is wrong
        assert!(check_file_sha1_hash(&tmpfile.path().to_path_buf(), "abcd123", 123).is_err());
    }

    #[test]
    fn parse_map_simple() {
        let result = parse_key_value_pairs("avg_time_res=0.5,avg_freq_res=10");
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.get("avg_time_res"), Some(&"0.5"));
        assert_eq!(map.get("avg_freq_res"), Some(&"10"));
    }

    #[test]
    fn parse_map_complex() {
        let result = parse_key_value_pairs(
            r#"avg_time_res=0.5 ,

            avg_freq_res = 10 "#,
        );
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.get("avg_time_res"), Some(&"0.5"));
        assert_eq!(map.get("avg_freq_res"), Some(&"10"));
    }

    #[test]
    fn bad_parse_map() {
        let result = parse_key_value_pairs("avg_time_res=0.5=1,avg_freq_res=10");
        assert!(result.is_err());

        let result = parse_key_value_pairs("avg_time_res=0.5,avg_freq_res");
        assert!(result.is_err());
    }
}
