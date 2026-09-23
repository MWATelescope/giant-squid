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
