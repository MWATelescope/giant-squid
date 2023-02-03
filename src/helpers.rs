// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small helper utility functions.

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

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
