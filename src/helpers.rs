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
use crate::obs_id::ObsId;

enum ObsIdOrJobId {
    /// This is an obsid.
    O(ObsId),
    /// This is a job ID.
    J(AsvoJobId),
}

fn parse_job_id_or_obs_id(s: &str) -> Option<ObsIdOrJobId> {
    match s.parse::<u64>() {
        // We successfully parsed an int.
        Ok(i) => {
            match ObsId::validate(i) {
                // This int is an obsid.
                Ok(o) => Some(ObsIdOrJobId::O(o)),
                // This int isn't an obsid; assume it is a jobid.
                Err(_) => Some(ObsIdOrJobId::J(i as AsvoJobId)),
            }
        }
        // Could not parse the string as an int; we must fail.
        Err(_) => None,
    }
}

/// Read a file, and return two vectors of ASVO job IDs and obsids. Fail if any
/// string in the file cannot be parsed as either.
pub fn parse_job_ids_and_obs_ids_from_file<T: AsRef<Path>>(
    f: T,
) -> Result<(Vec<AsvoJobId>, Vec<ObsId>), ParseError> {
    let mut obs_ids = vec![];
    let mut job_ids = vec![];

    // An IO error keeps the path, so the caller can say which file failed.
    let io_error = |source: io::Error| ParseError::IO {
        file: f.as_ref().to_path_buf(),
        source,
    };

    // Open the file.
    let mut reader = std::io::BufReader::new(std::fs::File::open(&f).map_err(io_error)?);
    let mut line = String::new();
    // For each line...
    while reader.read_line(&mut line).map_err(io_error)? > 0 {
        // ... split the whitespace and try to parse
        // obsids. Fail if whitespace-delimited text
        // can't be parsed into an int.
        for text in line.split_whitespace() {
            match parse_job_id_or_obs_id(text) {
                Some(ObsIdOrJobId::O(obs_id)) => obs_ids.push(obs_id),
                Some(ObsIdOrJobId::J(job_id)) => job_ids.push(job_id),
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

    Ok((job_ids, obs_ids))
}

/// Parse a string of ASVO job IDs, obsids, or files containing job IDs or
/// obsids into two vectors of job IDs and obsids.
pub fn parse_many_job_ids_or_obs_ids(
    strings: &[String],
) -> Result<(Vec<AsvoJobId>, Vec<ObsId>), ParseError> {
    // Attempt to parse all arguments as ints. If they aren't 10
    // digits long, assume they are ASVO job IDs. If any argument is
    // not an int, assume it is a file. Exit on any error.
    let mut job_ids = vec![];
    let mut obs_ids = vec![];
    for s in strings {
        match parse_job_id_or_obs_id(s) {
            Some(ObsIdOrJobId::O(obs_id)) => obs_ids.push(obs_id),
            Some(ObsIdOrJobId::J(job_id)) => job_ids.push(job_id),
            // Could not parse the string as an int; assume it is a
            // file and unpack it.
            None => {
                let (mut j, mut o) = parse_job_ids_and_obs_ids_from_file(s)?;
                job_ids.append(&mut j);
                obs_ids.append(&mut o);
            }
        }
    }

    Ok((job_ids, obs_ids))
}

#[derive(Error, Debug)]
pub enum ParseError {
    /// When a whitespace-delimited string inside a file isn't an integer, this
    /// error can be used.
    #[error("'{text}' in file {file} could not be parsed as an int.")]
    InsideFile { file: String, text: String },

    /// A file of job IDs and obsids could not be read.
    #[error("{}: {source}", file.display())]
    IO {
        /// The file.
        file: PathBuf,
        /// The error from the operating system.
        source: std::io::Error,
    },
}

/// Takes a filename, expected hash and a job id and returns
/// Ok if the calculated hash matches the expected hash, otherwise
/// returns an AsvoError::HashMismatch
pub fn check_file_sha1_hash(
    filename: &PathBuf,
    expected_hash: &str,
    job_id: AsvoJobId,
) -> Result<(), AsvoError> {
    let mut file = fs::File::open(filename)?;
    let mut hasher = Sha1::new();
    io::copy(&mut file, &mut hasher)?;
    let hash = format!("{:x}", hasher.finalize());

    if hash.eq_ignore_ascii_case(expected_hash) {
        Ok(())
    } else {
        Err(AsvoError::HashMismatch {
            job_id,
            file: filename.display().to_string(),
            calculated_hash: hash,
            expected_hash: expected_hash.to_string(),
        })
    }
}

#[cfg(test)]
mod test;
