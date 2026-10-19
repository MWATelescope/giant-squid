// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Making the files and directories of a download without following a
//! symbolic link.
//!
//! A download checks the paths that it writes to before it writes (a link
//! that is already in the download directory is an error). A check and the
//! write that follows are two steps, so another process could put a link in
//! the place between them, and the write would go wherever the link points.
//! Here the refusal is part of the open call itself (`O_NOFOLLOW`), so
//! there is no gap. On a system that is not Unix, the check is made just
//! before the open, so a small gap stays.
//!
//! [`open_for_writing`] covers the last part of a path. For the files of a
//! tar archive, [`DownloadRoot`] also covers the directories on the way: it
//! holds the download directory open (it may be a link), and walks down from
//! it one part at a time with `openat` and `O_NOFOLLOW`, making a directory
//! that is missing with `mkdirat`. A link on the way is refused at the step
//! that meets it, so none can be swapped in between a check and the use.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// How a file is opened for writing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WriteMode {
    /// Create the file, or empty it if it is there.
    CreateOrTruncate,
    /// Write after the end of the file, which must be there.
    Append,
    /// Write after the end of the file, which is created if it is not there.
    CreateOrAppend,
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

/// Why a file or directory below the download directory could not be made.
#[derive(Debug)]
pub(super) enum BelowError {
    /// This path, on the way or at the end, is a symbolic link.
    Symlink(PathBuf),
    /// Something else failed at this path.
    Io { path: PathBuf, source: io::Error },
}

/// The download directory, held open, for making files and directories below
/// it without following a symbolic link. The directory itself may be a link:
/// it is followed once, when it is opened.
pub(super) struct DownloadRoot(imp::Root);

impl DownloadRoot {
    /// Open the download directory `path`.
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        imp::Root::open(path).map(Self)
    }

    /// Open the file at `relative` (a path of normal parts, below the
    /// download directory) for writing as `mode` says, and make the
    /// directories on the way that are not there.
    pub(super) fn open_file(&self, relative: &Path, mode: WriteMode) -> Result<File, BelowError> {
        self.0.open_file(relative, mode)
    }

    /// Make the directory at `relative`, and the directories on the way, as
    /// far as they are not there.
    pub(super) fn create_dir_all(&self, relative: &Path) -> Result<(), BelowError> {
        self.0.create_dir_all(relative)
    }
}

/// The normal parts of `relative`, or an error for a path that is not below
/// the download directory (the caller has refused such a path before).
fn normal_parts(relative: &Path, root: &Path) -> Result<Vec<std::ffi::OsString>, BelowError> {
    use std::path::Component;

    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BelowError::Io {
                    path: root.join(relative),
                    source: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the path is not below the download directory",
                    ),
                });
            }
        }
    }
    Ok(parts)
}

#[cfg(unix)]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
    use std::path::{Path, PathBuf};

    use rustix::fs::{mkdirat, openat, statat, AtFlags, FileType, Mode, OFlags, CWD};
    use rustix::io::Errno;

    use super::{normal_parts, BelowError, WriteMode};

    /// The permissions of a new file, before the umask: read and write for
    /// everyone (0o666), as `File::create`. The permission bits are rustix's
    /// own, not a number: the type of a raw mode differs between platforms
    /// (`u32` on Linux, `u16` on macOS).
    fn new_file_mode() -> Mode {
        Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::WGRP | Mode::ROTH | Mode::WOTH
    }

    /// The permissions of a new directory, before the umask: all for
    /// everyone (0o777), as `mkdir`.
    fn new_dir_mode() -> Mode {
        Mode::RWXU | Mode::RWXG | Mode::RWXO
    }

    fn write_flags(mode: WriteMode) -> OFlags {
        OFlags::WRONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | match mode {
                WriteMode::CreateOrTruncate => OFlags::CREATE | OFlags::TRUNC,
                WriteMode::Append => OFlags::APPEND,
                WriteMode::CreateOrAppend => OFlags::CREATE | OFlags::APPEND,
            }
    }

    pub(super) fn open_for_writing(path: &Path, mode: WriteMode) -> io::Result<File> {
        let fd = openat(CWD, path, write_flags(mode), new_file_mode())?;
        Ok(File::from(fd))
    }

    /// Whether `errno` is what opening a symbolic link with `O_NOFOLLOW`
    /// gives. Linux, macOS and the other BSDs give ELOOP (or ENOTDIR with
    /// `O_DIRECTORY`); FreeBSD gives EMLINK.
    fn is_refusal_errno(errno: Errno) -> bool {
        errno == Errno::LOOP || errno == Errno::MLINK || errno == Errno::NOTDIR
    }

    pub(super) fn refused_a_symlink(error: &io::Error, path: &Path) -> bool {
        // ELOOP also means too many links on the way to the file.
        let refused = [Errno::LOOP, Errno::MLINK]
            .iter()
            .any(|errno| error.raw_os_error() == Some(errno.raw_os_error()));
        refused
            && std::fs::symlink_metadata(path)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
    }

    /// Whether `name` in the directory `parent` is a symbolic link.
    fn is_symlink_at(parent: BorrowedFd<'_>, name: &OsStr) -> bool {
        statat(parent, name, AtFlags::SYMLINK_NOFOLLOW)
            .is_ok_and(|stat| FileType::from_raw_mode(stat.st_mode).is_symlink())
    }

    /// The error of a failed step at `name` in `parent`, which is at `path`:
    /// a symbolic link, or another failure.
    fn step_error(parent: BorrowedFd<'_>, name: &OsStr, path: PathBuf, errno: Errno) -> BelowError {
        if is_refusal_errno(errno) && is_symlink_at(parent, name) {
            BelowError::Symlink(path)
        } else {
            BelowError::Io {
                path,
                source: errno.into(),
            }
        }
    }

    pub(super) struct Root {
        fd: OwnedFd,
        path: PathBuf,
    }

    impl Root {
        pub(super) fn open(path: &Path) -> io::Result<Self> {
            let fd = openat(
                CWD,
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            Ok(Self {
                fd,
                path: path.to_path_buf(),
            })
        }

        /// Walk down from the download directory through `parts`, making the
        /// directories that are not there, and give the last one (the
        /// download directory itself for no parts).
        fn descend(&self, parts: &[OsString]) -> Result<OwnedFd, BelowError> {
            let mut current: Option<OwnedFd> = None;
            let mut path = self.path.clone();
            for part in parts {
                path.push(part);
                let parent = current.as_ref().map_or(self.fd.as_fd(), AsFd::as_fd);
                let next = open_or_make_dir(parent, part, &path)?;
                current = Some(next);
            }
            match current {
                Some(fd) => Ok(fd),
                None => self.fd.try_clone().map_err(|source| BelowError::Io {
                    path: self.path.clone(),
                    source,
                }),
            }
        }

        pub(super) fn open_file(
            &self,
            relative: &Path,
            mode: WriteMode,
        ) -> Result<File, BelowError> {
            let mut parts = normal_parts(relative, &self.path)?;
            let path = self.path.join(relative);
            let Some(name) = parts.pop() else {
                return Err(BelowError::Io {
                    path,
                    source: io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"),
                });
            };
            let dir = self.descend(&parts)?;
            openat(&dir, &name, write_flags(mode), new_file_mode())
                .map(File::from)
                .map_err(|errno| step_error(dir.as_fd(), &name, path, errno))
        }

        pub(super) fn create_dir_all(&self, relative: &Path) -> Result<(), BelowError> {
            let parts = normal_parts(relative, &self.path)?;
            self.descend(&parts).map(drop)
        }
    }

    /// Open the directory `name` in `parent` (which is at `path`, with
    /// `name`), making it first if it is not there. A symbolic link is not
    /// followed.
    fn open_or_make_dir(
        parent: BorrowedFd<'_>,
        name: &OsString,
        path: &Path,
    ) -> Result<OwnedFd, BelowError> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let open = || openat(parent, name, flags, Mode::empty());
        match open() {
            Ok(fd) => Ok(fd),
            Err(Errno::NOENT) => {
                // Another process may make it between the two calls: that is
                // fine, it is opened next.
                match mkdirat(parent, name, new_dir_mode()) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(errno) => {
                        return Err(BelowError::Io {
                            path: path.to_path_buf(),
                            source: errno.into(),
                        })
                    }
                }
                open().map_err(|errno| step_error(parent, name, path.to_path_buf(), errno))
            }
            Err(errno) => Err(step_error(parent, name, path.to_path_buf(), errno)),
        }
    }
}

#[cfg(not(unix))]
mod imp {
    use std::fs::File;
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{normal_parts, BelowError, WriteMode};

    pub(super) fn open_for_writing(path: &Path, mode: WriteMode) -> io::Result<File> {
        if is_a_symlink(path) {
            return Err(io::Error::other("the path is a symbolic link"));
        }
        match mode {
            WriteMode::CreateOrTruncate => File::create(path),
            WriteMode::Append => File::options().append(true).open(path),
            WriteMode::CreateOrAppend => File::options().create(true).append(true).open(path),
        }
    }

    /// The download directory. Off Unix, each part is checked just before it
    /// is used, so a small gap stays.
    pub(super) struct Root {
        path: PathBuf,
    }

    impl Root {
        pub(super) fn open(path: &Path) -> io::Result<Self> {
            std::fs::metadata(path)?;
            Ok(Self {
                path: path.to_path_buf(),
            })
        }

        fn descend(&self, parts: &[std::ffi::OsString]) -> Result<PathBuf, BelowError> {
            let mut path = self.path.clone();
            for part in parts {
                path.push(part);
                if is_a_symlink(&path) {
                    return Err(BelowError::Symlink(path));
                }
                match std::fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => {}
                    Err(source) => return Err(BelowError::Io { path, source }),
                }
            }
            Ok(path)
        }

        pub(super) fn open_file(
            &self,
            relative: &Path,
            mode: WriteMode,
        ) -> Result<File, BelowError> {
            let mut parts = normal_parts(relative, &self.path)?;
            let path = self.path.join(relative);
            let Some(name) = parts.pop() else {
                return Err(BelowError::Io {
                    path,
                    source: io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"),
                });
            };
            let dir = self.descend(&parts)?;
            open_for_writing(&dir.join(name), mode).map_err(|source| {
                if is_a_symlink(&path) {
                    BelowError::Symlink(path)
                } else {
                    BelowError::Io { path, source }
                }
            })
        }

        pub(super) fn create_dir_all(&self, relative: &Path) -> Result<(), BelowError> {
            let parts = normal_parts(relative, &self.path)?;
            self.descend(&parts).map(drop)
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
