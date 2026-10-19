// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io::Write;

use tempfile::TempDir;

use super::{open_for_writing, refused_a_symlink, WriteMode};

#[test]
fn a_new_file_is_created() {
    let dir = TempDir::new().expect("could not create a directory");
    let path = dir.path().join("new.dat");

    let mut file = open_for_writing(&path, WriteMode::CreateOrTruncate).expect("the file");
    file.write_all(b"data").expect("could not write");
    drop(file);

    assert_eq!(std::fs::read(&path).expect("the file"), b"data");
}

#[test]
fn an_existing_file_is_emptied() {
    let dir = TempDir::new().expect("could not create a directory");
    let path = dir.path().join("old.dat");
    std::fs::write(&path, b"old content").expect("could not write the file");

    drop(open_for_writing(&path, WriteMode::CreateOrTruncate).expect("the file"));

    assert_eq!(std::fs::read(&path).expect("the file"), b"");
}

#[test]
fn append_writes_after_the_end() {
    let dir = TempDir::new().expect("could not create a directory");
    let path = dir.path().join("part.dat");
    std::fs::write(&path, b"start-").expect("could not write the file");

    let mut file = open_for_writing(&path, WriteMode::Append).expect("the file");
    file.write_all(b"end").expect("could not write");
    drop(file);

    assert_eq!(std::fs::read(&path).expect("the file"), b"start-end");
}

#[test]
fn append_does_not_create_a_file() {
    let dir = TempDir::new().expect("could not create a directory");
    let path = dir.path().join("missing.dat");

    let err = open_for_writing(&path, WriteMode::Append).expect_err("there is no file");

    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(!refused_a_symlink(&err, &path));
    assert!(!path.exists());
}

/// A link to a file is refused, in both modes, and the file is not changed.
#[cfg(unix)]
#[test]
fn a_link_to_a_file_is_refused_and_the_file_is_not_changed() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let target = elsewhere.path().join("victim.dat");
    std::fs::write(&target, b"untouched").expect("could not write the target");
    let link = dir.path().join("link.dat");
    std::os::unix::fs::symlink(&target, &link).expect("could not make the link");

    for mode in [WriteMode::CreateOrTruncate, WriteMode::Append] {
        let err = open_for_writing(&link, mode).expect_err("the link should be refused");
        assert!(refused_a_symlink(&err, &link), "{mode:?}: {err:?}");
        assert_eq!(std::fs::read(&target).expect("the target"), b"untouched");
    }
}

/// A link that points nowhere is refused too, and the place it points to is
/// not created.
#[cfg(unix)]
#[test]
fn a_dangling_link_is_refused_and_its_target_is_not_created() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let target = elsewhere.path().join("missing.dat");
    let link = dir.path().join("link.dat");
    std::os::unix::fs::symlink(&target, &link).expect("could not make the link");

    let err = open_for_writing(&link, WriteMode::CreateOrTruncate)
        .expect_err("the link should be refused");

    assert!(refused_a_symlink(&err, &link), "{err:?}");
    assert!(!target.exists(), "nothing may be created through the link");
}

/// A link to a directory is refused, as a link to a file is.
#[cfg(unix)]
#[test]
fn a_link_to_a_directory_is_refused() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(elsewhere.path(), &link).expect("could not make the link");

    let err = open_for_writing(&link, WriteMode::CreateOrTruncate)
        .expect_err("the link should be refused");

    assert!(refused_a_symlink(&err, &link), "{err:?}");
}

/// A link on the way to the file (not at its end) is followed: the download
/// directory itself may be a link.
#[cfg(unix)]
#[test]
fn a_link_before_the_last_part_is_followed() {
    let dir = TempDir::new().expect("could not create a directory");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).expect("could not create the directory");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("could not make the link");

    drop(open_for_writing(&link.join("a.dat"), WriteMode::CreateOrTruncate).expect("the file"));

    assert!(real.join("a.dat").is_file());
}
