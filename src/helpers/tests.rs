// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for [`super`] helper functions.

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
fn a_missing_file_of_ids_names_the_file() {
    let missing = std::env::temp_dir().join("giant-squid-no-such-file-of-ids.txt");

    let err = parse_job_ids_and_obs_ids_from_file(&missing).expect_err("the file does not exist");

    match &err {
        ParseError::IO { file, source } => {
            assert_eq!(file, &missing);
            assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        }
        other => panic!("expected an IO error, got {other:?}"),
    }
    assert!(
        err.to_string().contains(&missing.display().to_string()),
        "the message should name the file: {err}"
    );
}

/// An obsid, and another, used by the guard tests.
const OBS_ID_A: &str = "1065880128";
const OBS_ID_B: &str = "1065880248";
/// A job ID: a number that is not a valid obsid.
const JOB_ID_TEXT: &str = "31";

/// The strings of a list of arguments.
fn strings(texts: &[&str]) -> Vec<String> {
    texts.iter().map(ToString::to_string).collect()
}

/// A file of whitespace-separated numbers.
fn file_of(contents: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("a temporary file");
    write!(file, "{contents}").expect("write the file");
    file.flush().expect("flush the file");
    file
}

#[test]
fn obsids_only_returns_the_obsids_in_order() {
    let obs_ids = parse_obs_ids_only(&strings(&[OBS_ID_B, OBS_ID_A])).expect("obsids");

    let numbers: Vec<u64> = obs_ids.into_iter().map(u64::from).collect();
    assert_eq!(numbers, [1065880248, 1065880128]);
}

#[test]
fn obsids_only_reads_a_file_of_obsids() {
    let file = file_of(&format!("{OBS_ID_A}\n{OBS_ID_B}\n"));
    let path = file.path().display().to_string();

    assert_eq!(parse_obs_ids_only(&[path]).expect("the file").len(), 2);
}

/// A job ID is refused even when obsids are given too, and the message names
/// the job IDs as job IDs (it once said "exceptions").
#[test]
fn obsids_only_refuses_a_job_id_and_names_it() {
    let err = parse_obs_ids_only(&strings(&[OBS_ID_A, JOB_ID_TEXT])).expect_err("a job ID");

    assert!(matches!(&err, ParseError::JobIdsGiven { job_ids } if job_ids == &[31]));
    assert_eq!(
        err.to_string(),
        "Expected only obsids, but found these job IDs: [31]"
    );
}

#[test]
fn obsids_only_refuses_nothing_at_all() {
    let err = parse_obs_ids_only(&[]).expect_err("no obsid");

    assert!(matches!(err, ParseError::NoObsIds));
    assert_eq!(err.to_string(), "No obsids specified!");
}

#[test]
fn job_ids_only_returns_the_job_ids_in_order() {
    let job_ids = parse_job_ids_only(&strings(&[JOB_ID_TEXT, "7"])).expect("job IDs");

    assert_eq!(job_ids, [31, 7]);
}

#[test]
fn job_ids_only_refuses_an_obsid_and_names_all_of_them() {
    let err = parse_job_ids_only(&strings(&[OBS_ID_A, JOB_ID_TEXT, OBS_ID_B])).expect_err("obsids");

    assert!(matches!(&err, ParseError::ObsIdsGiven { obs_ids } if obs_ids.len() == 2));
    assert_eq!(
        err.to_string(),
        format!(
            "Expected only job IDs, but found these obsids: {OBS_ID_A}, {OBS_ID_B}. {OBS_ID_HINT}"
        )
    );
}

#[test]
fn job_ids_only_refuses_nothing_at_all() {
    let err = parse_job_ids_only(&[]).expect_err("no job ID");

    assert!(matches!(err, ParseError::NoJobIds));
    assert_eq!(err.to_string(), "No jobids specified!");
}

#[test]
fn the_guards_report_a_file_that_cannot_be_read() {
    let missing = strings(&["/no/such/file/of/ids"]);

    assert!(matches!(
        parse_obs_ids_only(&missing),
        Err(ParseError::IO { .. })
    ));
    assert!(matches!(
        parse_job_ids_only(&missing),
        Err(ParseError::IO { .. })
    ));
}

/// A date is midnight UTC, and an RFC 3339 time keeps its instant.
#[test]
fn a_time_is_a_date_or_rfc_3339() {
    let midnight: jiff::Timestamp = "2026-09-01T00:00:00Z".parse().expect("a time");

    assert_eq!(parse_utc_time("2026-09-01").expect("a date"), midnight);
    assert_eq!(
        parse_utc_time("2026-09-01T00:00:00Z").expect("UTC"),
        midnight
    );
    assert_eq!(
        parse_utc_time("2026-09-01T00:00:00z").expect("a z"),
        midnight
    );
    assert_eq!(
        parse_utc_time("2026-09-01T08:00:00+08:00").expect("an offset"),
        midnight
    );
}

/// A time with no offset is refused rather than guessed.
#[test]
fn a_time_without_an_offset_is_refused() {
    for text in ["2026-09-01T10:00:00", "2026-9-1x", "yesterday", ""] {
        let err = parse_utc_time(text).expect_err(text);
        assert!(matches!(err, ParseError::InvalidTime), "{text}");
    }
    assert_eq!(
        ParseError::InvalidTime.to_string(),
        "not a time: use RFC 3339 (2026-09-01T00:00:00Z) or a date (2026-09-01)"
    );
}
