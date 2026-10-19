// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io::Write;

use tempfile::TempDir;

use std::path::Path;

use super::{open_for_writing, refused_a_symlink, BelowError, DownloadRoot, WriteMode};

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

// ---------------------------------------------------------------------------
// DownloadRoot
// ---------------------------------------------------------------------------

fn root_of(dir: &TempDir) -> DownloadRoot {
    DownloadRoot::open(dir.path()).expect("could not open the download directory")
}

#[test]
fn a_file_is_made_with_the_directories_on_the_way() {
    let dir = TempDir::new().expect("could not create a directory");

    let mut file = root_of(&dir)
        .open_file(Path::new("a/b/c.dat"), WriteMode::CreateOrTruncate)
        .expect("the file");
    file.write_all(b"data").expect("could not write");
    drop(file);

    assert_eq!(
        std::fs::read(dir.path().join("a").join("b").join("c.dat")).expect("the file"),
        b"data"
    );
}

#[test]
fn a_current_dir_part_is_ignored_and_existing_directories_are_reused() {
    let dir = TempDir::new().expect("could not create a directory");
    std::fs::create_dir(dir.path().join("a")).expect("could not create a directory");
    std::fs::write(dir.path().join("a").join("old.dat"), b"old").expect("could not write");

    let root = root_of(&dir);
    drop(
        root.open_file(Path::new("./a/./new.dat"), WriteMode::CreateOrTruncate)
            .expect("the file"),
    );
    drop(
        root.open_file(Path::new("a/old.dat"), WriteMode::CreateOrTruncate)
            .expect("the file"),
    );

    assert!(dir.path().join("a").join("new.dat").is_file());
    assert_eq!(
        std::fs::read(dir.path().join("a").join("old.dat")).expect("old.dat"),
        b""
    );
}

#[test]
fn create_or_append_makes_the_file_and_then_adds_to_it() {
    let dir = TempDir::new().expect("could not create a directory");
    let root = root_of(&dir);

    for text in [b"one-".as_slice(), b"two".as_slice()] {
        let mut file = root
            .open_file(Path::new("sub/part.dat"), WriteMode::CreateOrAppend)
            .expect("the file");
        file.write_all(text).expect("could not write");
    }

    assert_eq!(
        std::fs::read(dir.path().join("sub").join("part.dat")).expect("the file"),
        b"one-two"
    );
}

#[test]
fn directories_are_made_with_their_parents() {
    let dir = TempDir::new().expect("could not create a directory");
    let root = root_of(&dir);

    root.create_dir_all(Path::new("a/b/c"))
        .expect("the directories");
    root.create_dir_all(Path::new("a/b"))
        .expect("the directories again");
    root.create_dir_all(Path::new("")).expect("no directories");

    assert!(dir.path().join("a").join("b").join("c").is_dir());
}

#[test]
fn a_path_that_is_not_below_the_download_directory_is_refused() {
    let dir = TempDir::new().expect("could not create a directory");
    let root = root_of(&dir);

    for path in ["../escaped.dat", "/etc/passwd", "a/../../escaped.dat"] {
        let err = root
            .open_file(Path::new(path), WriteMode::CreateOrTruncate)
            .expect_err("the path should be refused");
        assert!(matches!(err, BelowError::Io { .. }), "{path}: {err:?}");
    }
    assert!(!dir
        .path()
        .parent()
        .expect("a parent")
        .join("escaped.dat")
        .exists());
}

#[test]
fn a_file_where_a_directory_is_needed_is_an_error_but_not_a_link() {
    let dir = TempDir::new().expect("could not create a directory");
    std::fs::write(dir.path().join("a"), b"a file").expect("could not write");
    let root = root_of(&dir);

    let err = root
        .open_file(Path::new("a/b.dat"), WriteMode::CreateOrTruncate)
        .expect_err("a is a file");
    assert!(matches!(err, BelowError::Io { .. }), "{err:?}");

    let err = root
        .create_dir_all(Path::new("a/b"))
        .expect_err("a is a file");
    assert!(matches!(err, BelowError::Io { .. }), "{err:?}");
}

/// A link to a directory on the way is refused, with the link in the error,
/// and nothing is made where it points.
#[cfg(unix)]
#[test]
fn a_link_on_the_way_is_refused_and_nothing_is_made_through_it() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let link = dir.path().join("sub");
    std::os::unix::fs::symlink(elsewhere.path(), &link).expect("could not make the link");
    let root = root_of(&dir);

    let err = root
        .open_file(Path::new("sub/deeper/a.dat"), WriteMode::CreateOrTruncate)
        .expect_err("the link should be refused");
    assert!(
        matches!(&err, BelowError::Symlink(path) if *path == link),
        "{err:?}"
    );

    let err = root
        .create_dir_all(Path::new("sub/deeper"))
        .expect_err("the link should be refused");
    assert!(
        matches!(&err, BelowError::Symlink(path) if *path == link),
        "{err:?}"
    );

    assert_eq!(
        std::fs::read_dir(elsewhere.path())
            .expect("elsewhere")
            .count(),
        0,
        "nothing may be made through the link"
    );
}

/// A link deeper down is refused too, and a link at the end of the path.
#[cfg(unix)]
#[test]
fn a_link_deeper_down_or_at_the_end_is_refused() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let victim = elsewhere.path().join("victim.dat");
    std::fs::write(&victim, b"untouched").expect("could not write");
    std::fs::create_dir(dir.path().join("a")).expect("could not create a directory");
    let deep_link = dir.path().join("a").join("link");
    std::os::unix::fs::symlink(elsewhere.path(), &deep_link).expect("could not make the link");
    let file_link = dir.path().join("a").join("file_link");
    std::os::unix::fs::symlink(&victim, &file_link).expect("could not make the link");
    let root = root_of(&dir);

    let err = root
        .open_file(Path::new("a/link/x.dat"), WriteMode::CreateOrTruncate)
        .expect_err("the link should be refused");
    assert!(
        matches!(&err, BelowError::Symlink(path) if *path == deep_link),
        "{err:?}"
    );

    let err = root
        .open_file(Path::new("a/file_link"), WriteMode::CreateOrTruncate)
        .expect_err("the link should be refused");
    assert!(
        matches!(&err, BelowError::Symlink(path) if *path == file_link),
        "{err:?}"
    );
    assert_eq!(std::fs::read(&victim).expect("the target"), b"untouched");
}

/// A link that points nowhere is refused as a link, not made into a
/// directory.
#[cfg(unix)]
#[test]
fn a_dangling_link_on_the_way_is_refused() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let missing = elsewhere.path().join("missing");
    let link = dir.path().join("sub");
    std::os::unix::fs::symlink(&missing, &link).expect("could not make the link");

    let err = root_of(&dir)
        .create_dir_all(Path::new("sub"))
        .expect_err("the link should be refused");

    assert!(
        matches!(&err, BelowError::Symlink(path) if *path == link),
        "{err:?}"
    );
    assert!(!missing.exists());
}

/// The download directory itself may be a link: it is followed once.
#[cfg(unix)]
#[test]
fn the_download_directory_may_be_a_link() {
    let dir = TempDir::new().expect("could not create a directory");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).expect("could not create the directory");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("could not make the link");

    let root = DownloadRoot::open(&link).expect("the link to a directory");
    drop(
        root.open_file(Path::new("sub/a.dat"), WriteMode::CreateOrTruncate)
            .expect("the file"),
    );

    assert!(real.join("sub").join("a.dat").is_file());
}

/// The handle keeps pointing at the directory that was opened, so a rename
/// of the download directory afterwards does not move the writes: what is
/// written goes to the directory that was opened.
#[cfg(unix)]
#[test]
fn the_open_directory_is_kept_if_its_path_is_swapped_for_a_link() {
    let dir = TempDir::new().expect("could not create a directory");
    let elsewhere = TempDir::new().expect("could not create a second directory");
    let download = dir.path().join("download");
    std::fs::create_dir(&download).expect("could not create the directory");
    let root = DownloadRoot::open(&download).expect("the directory");

    // Another process swaps the path for a link to another directory.
    let moved = dir.path().join("moved");
    std::fs::rename(&download, &moved).expect("could not rename");
    std::os::unix::fs::symlink(elsewhere.path(), &download).expect("could not make the link");

    drop(
        root.open_file(Path::new("a.dat"), WriteMode::CreateOrTruncate)
            .expect("the file"),
    );

    assert!(moved.join("a.dat").is_file(), "written where it was opened");
    assert!(!elsewhere.path().join("a.dat").exists());
}

/// A new file and a new directory have the permissions of `File::create` and
/// `mkdir` (before the umask): at least the owner can read and write the file
/// and use the directory.
#[cfg(unix)]
#[test]
fn new_files_and_directories_are_usable_by_their_owner() {
    use std::os::unix::fs::PermissionsExt;

    const OWNER_READ_WRITE: u32 = 0o600;
    const OWNER_ALL: u32 = 0o700;

    let dir = TempDir::new().expect("could not create a directory");
    drop(
        root_of(&dir)
            .open_file(Path::new("sub/a.dat"), WriteMode::CreateOrTruncate)
            .expect("the file"),
    );

    let file_mode = std::fs::metadata(dir.path().join("sub").join("a.dat"))
        .expect("the file")
        .permissions()
        .mode();
    let dir_mode = std::fs::metadata(dir.path().join("sub"))
        .expect("the directory")
        .permissions()
        .mode();
    assert_eq!(
        file_mode & OWNER_READ_WRITE,
        OWNER_READ_WRITE,
        "{file_mode:o}"
    );
    assert_eq!(dir_mode & OWNER_ALL, OWNER_ALL, "{dir_mode:o}");
}
