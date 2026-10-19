// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Opening a file for writing without following a symbolic link at the last
//! part of its path.
//!
//! A download checks the paths that it writes to before it writes (a link
//! that is already in the download directory is an error). A check and the
//! write that follows are two steps, so another process could put a link in
//! the place between them, and the write would go wherever the link points.
//! Here the refusal is part of the open call itself (`O_NOFOLLOW`), so
//! there is no gap. On a system that is not Unix, the check is made just
//! before the open, so a small gap stays.
//!
//! `O_NOFOLLOW` covers the last part of the path only. The parts before it
//! are the download directory (which may be a link) and the directories of a
//! tar entry (see [`super::symlink_on_the_way`]).

use std::fs::File;
use std::io;
use std::path::Path;

/// How a file is opened for writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WriteMode {
    /// Create the file, or empty it if it is there.
    CreateOrTruncate,
    /// Write after the end of the file, which must be there.
    Append,
}

/// Open `path` for writing as `mode` says, and refuse a symbolic link at
/// `path` (also one that points nowhere): the open fails, and nothing is
/// created or changed where the link points. Use [`refused_a_symlink`] to tell
/// this failure from another.
pub(super) fn open_for_writing(path: &Path, mode: WriteMode) -> io::Result<File> {
    imp::open_for_writing(path, mode)
}

/// Whether `error`, from [`open_for_writing`] of `path`, is the refusal of a
/// symbolic link.
pub(super) fn refused_a_symlink(error: &io::Error, path: &Path) -> bool {
    imp::refused_a_symlink(error, path)
}

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    use rustix::fs::{openat, Mode, OFlags, CWD};
    use rustix::io::Errno;

    use super::WriteMode;

    /// The permissions of a new file, before the umask: as `File::create`.
    const NEW_FILE_MODE: u32 = 0o666;

    pub(super) fn open_for_writing(path: &Path, mode: WriteMode) -> io::Result<File> {
        let flags = OFlags::WRONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | match mode {
                WriteMode::CreateOrTruncate => OFlags::CREATE | OFlags::TRUNC,
                WriteMode::Append => OFlags::APPEND,
            };
        let fd = openat(CWD, path, flags, Mode::from_raw_mode(NEW_FILE_MODE))?;
        Ok(File::from(fd))
    }

    pub(super) fn refused_a_symlink(error: &io::Error, path: &Path) -> bool {
        // Linux, macOS and the other BSDs give ELOOP; FreeBSD gives EMLINK.
        let refused = [Errno::LOOP, Errno::MLINK]
            .iter()
            .any(|errno| error.raw_os_error() == Some(errno.raw_os_error()));
        // ELOOP also means too many links on the way to the file.
        refused
            && std::fs::symlink_metadata(path)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
    }
}

#[cfg(not(unix))]
mod imp {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    use super::WriteMode;

    pub(super) fn open_for_writing(path: &Path, mode: WriteMode) -> io::Result<File> {
        if is_a_symlink(path) {
            return Err(io::Error::other("the path is a symbolic link"));
        }
        match mode {
            WriteMode::CreateOrTruncate => File::create(path),
            WriteMode::Append => File::options().append(true).open(path),
        }
    }

    pub(super) fn refused_a_symlink(_error: &io::Error, path: &Path) -> bool {
        is_a_symlink(path)
    }

    fn is_a_symlink(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    }
}

#[cfg(test)]
mod tests;
