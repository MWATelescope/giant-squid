// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small helper utility functions.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::{fs, io};

use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::mwa_asvo::*;
use crate::obs_id::ObsId;

enum ObsIdOrJobId {
    /// This is an Obs ID.
    O(ObsId),
    /// This is a Job ID.
    J(AsvoJobId),
    /// This is 0, which is not a Job ID.
    Zero,
}

fn parse_job_id_or_obs_id(s: &str) -> Option<ObsIdOrJobId> {
    match s.parse::<u64>() {
        // We successfully parsed an int.
        Ok(i) => {
            match ObsId::validate(i) {
                // This int is an obsid.
                Ok(o) => Some(ObsIdOrJobId::O(o)),
                // This int isn't an obsid; assume it is a jobid.
                Err(_) => Some(AsvoJobId::new(i).map_or(ObsIdOrJobId::Zero, ObsIdOrJobId::J)),
            }
        }
        // Could not parse the string as an int; we must fail.
        Err(_) => None,
    }
}

/// Read a file, and return two vectors of ASVO Job IDs and Obs IDs. Fail if any
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
                Some(ObsIdOrJobId::Zero) => return Err(ParseError::ZeroJobId),
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

/// Parse a string of ASVO Job IDs, Obs IDs, or files containing Job IDs or
/// Obs IDs into two vectors of Job IDs and Obs IDs.
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
            Some(ObsIdOrJobId::Zero) => return Err(ParseError::ZeroJobId),
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

    /// A file of Job IDs and Obs IDs could not be read.
    #[error("{}: {source}", file.display())]
    IO {
        /// The file.
        file: PathBuf,
        /// The error from the operating system.
        source: std::io::Error,
    },

    /// A command that takes Obs IDs only was given Job IDs
    /// ([`parse_obs_ids_only`]).
    #[error(
        "Expected only Obs IDs, but found these Job IDs: {}",
        ids_text(job_ids)
    )]
    JobIdsGiven {
        /// The Job IDs, in the order given.
        job_ids: Vec<AsvoJobId>,
    },

    /// A command that takes Job IDs only was given Obs IDs
    /// ([`parse_job_ids_only`]).
    #[error(
        "Expected only Job IDs, but found these Obs IDs: {}. {OBS_ID_HINT}",
        ids_text(obs_ids)
    )]
    ObsIdsGiven {
        /// The Obs IDs, in the order given.
        obs_ids: Vec<ObsId>,
    },

    /// 0 was given: it is not an Obs ID, and the MWA ASVO's Job IDs start at 1.
    #[error("0 is not a Job ID or an Obs ID")]
    ZeroJobId,

    /// No Obs ID was given ([`parse_obs_ids_only`]).
    #[error("No Obs IDs specified.")]
    NoObsIds,

    /// No Job ID was given ([`parse_job_ids_only`]).
    #[error("No Job IDs specified.")]
    NoJobIds,

    /// Text is neither an RFC 3339 time nor a date ([`parse_utc_time`]).
    #[error("not a time: use RFC 3339 (2026-09-01T00:00:00Z) or a date (2026-09-01)")]
    InvalidTime,
}

/// What to do when an Obs ID is given to a command that takes Job IDs only.
pub const OBS_ID_HINT: &str = "To find the Job IDs of an Obs ID, use 'giant-squid list <OBS_ID>'.";

/// The date-only form that [`parse_utc_time`] accepts.
const DATE_ONLY_FORMAT: &str = "%Y-%m-%d";

/// The IDs as text for a message: `1065880128, 1065880248`.
fn ids_text<T: std::fmt::Display>(ids: &[T]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse Obs IDs and files of Obs IDs, for a command that takes Obs IDs only.
/// A file is read as [`parse_many_job_ids_or_obs_ids`] reads it.
///
/// # Errors
///
/// - A Job ID anywhere in the arguments (or in a file) is
///   [`ParseError::JobIdsGiven`], even when Obs IDs are also given. Ignoring
///   it would submit fewer jobs than the user asked for.
/// - No Obs ID at all is [`ParseError::NoObsIds`].
/// - A file that cannot be read or parsed is an error.
pub fn parse_obs_ids_only(strings: &[String]) -> Result<Vec<ObsId>, ParseError> {
    let (job_ids, obs_ids) = parse_many_job_ids_or_obs_ids(strings)?;
    if !job_ids.is_empty() {
        return Err(ParseError::JobIdsGiven { job_ids });
    }
    if obs_ids.is_empty() {
        return Err(ParseError::NoObsIds);
    }
    Ok(obs_ids)
}

/// Parse Job IDs and files of Job IDs, for a command that takes Job IDs only
/// (`wait` and `cancel`). A file is read as
/// [`parse_many_job_ids_or_obs_ids`] reads it.
///
/// # Errors
///
/// - An Obs ID anywhere in the arguments (or in a file) is
///   [`ParseError::ObsIdsGiven`], even when Job IDs are also given. Ignoring
///   it would wait for, or cancel, fewer jobs than the user asked for.
/// - No Job ID at all is [`ParseError::NoJobIds`].
/// - A file that cannot be read or parsed is an error.
pub fn parse_job_ids_only(strings: &[String]) -> Result<Vec<AsvoJobId>, ParseError> {
    let (job_ids, obs_ids) = parse_many_job_ids_or_obs_ids(strings)?;
    if !obs_ids.is_empty() {
        return Err(ParseError::ObsIdsGiven { obs_ids });
    }
    if job_ids.is_empty() {
        return Err(ParseError::NoJobIds);
    }
    Ok(job_ids)
}

/// Parse a time for the `date_from` and `date_to` of a job listing: RFC 3339
/// (for example `2026-09-01T00:00:00Z`), or a date alone (`2026-09-01`),
/// which is midnight UTC.
///
/// A date and time with no offset (`2026-09-01T12:00:00`) is refused rather
/// than guessed: the date form must be exactly `YYYY-MM-DD`.
///
/// # Errors
///
/// [`ParseError::InvalidTime`] for any other text.
pub fn parse_utc_time(text: &str) -> Result<jiff::Timestamp, ParseError> {
    if let Ok(time) = text.parse::<jiff::Timestamp>() {
        return Ok(time);
    }
    jiff::civil::Date::strptime(DATE_ONLY_FORMAT, text)
        .and_then(|date| date.to_zoned(jiff::tz::TimeZone::UTC))
        .map(|midnight| midnight.timestamp())
        .map_err(|_| ParseError::InvalidTime)
}

/// The size of the buffer that [`hash_reader`] reads through.
const HASH_READ_BUFFER_SIZE: usize = 1024 * 1024;

/// Lower-case hexadecimal of `bytes`, for example a SHA1 hash. The `sha1`
/// crate's output type has no `{:x}` format.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Add everything that `reader` gives to `hasher`, and return the number of
/// bytes. The `sha1` crate's hasher is not an `io::Write`, so `io::copy`
/// cannot do this.
pub(crate) fn hash_reader(mut reader: impl io::Read, hasher: &mut Sha1) -> io::Result<u64> {
    let mut buffer = vec![0; HASH_READ_BUFFER_SIZE];
    let mut total: u64 = 0;
    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hasher.update(&buffer[..n]);
        total += n as u64;
    }
}

/// Takes a filename, expected hash and a Job ID and returns
/// Ok if the calculated hash matches the expected hash, otherwise
/// returns an AsvoError::HashMismatch
pub fn check_file_sha1_hash(
    filename: &PathBuf,
    expected_hash: &str,
    job_id: AsvoJobId,
) -> Result<(), AsvoError> {
    let file = fs::File::open(filename)?;
    let mut hasher = Sha1::new();
    hash_reader(file, &mut hasher)?;
    let hash = to_hex(&hasher.finalize());

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
mod tests;
