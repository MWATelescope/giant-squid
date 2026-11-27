// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Local storage for MWA ASVO JWT session tokens.
//!
//! The file is at the same path as mwa-cli's own token cache
//! (`$HOME/.mwa-asvo/tokens.json`), and has mwa-cli's fields and two more
//! (see [`StoredTokens`]). giant-squid does not use a session that mwa-cli
//! wrote, because that has no host and no API key hash: it logs in again and
//! replaces the file.
//!
//! The caller supplies the cache path. [`default_token_cache_path`] gives the
//! path mwa-cli uses,
//! relative to a home directory that the caller finds.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jiff::Timestamp;
use log::debug;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::helpers::to_hex;

#[cfg(test)]
mod tests;

/// How much of a safety buffer to apply when deciding if a token is still
/// valid. This avoids a token expiring mid-flight between the check and the
/// request actually being sent.
const EXPIRY_SAFETY_BUFFER: Duration = Duration::from_secs(30);

/// The token cache directory, relative to the home directory. Shared with
/// mwa-cli.
const TOKEN_CACHE_DIR: &str = ".mwa-asvo";
/// The token cache file name, inside [`TOKEN_CACHE_DIR`]. Shared with
/// mwa-cli.
const TOKEN_CACHE_FILE: &str = "tokens.json";

/// The mode of the token cache file: the user can read and write it, and
/// nobody else can.
#[cfg(unix)]
const TOKEN_FILE_MODE: u32 = 0o600;
/// The mode of a token cache directory that giant-squid creates.
#[cfg(unix)]
const TOKEN_DIR_MODE: u32 = 0o700;
/// The permission bits of a mode, without the file type.
#[cfg(unix)]
const PERMISSION_BITS: u32 = 0o777;
/// The permission bits of the group and of others.
#[cfg(unix)]
const GROUP_AND_OTHER_BITS: u32 = 0o077;

/// The end of the name of the temporary file that a save writes first.
const TEMPORARY_SUFFIX: &str = ".tmp";
/// Makes the names of the temporary files of the saves in one process
/// different from each other, for the threads that save at the same time.
static TEMPORARY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A cached MWA ASVO JWT session, as persisted to disk.
///
/// The fields up to `user_email` are those of mwa-cli's own `tokens.json`.
/// `host` and `api_key_sha256` are added by giant-squid: a session is for
/// one host and one API key, and the file must say which, or a client with
/// another key (or another host) would use the session of someone else.
/// A file without them (one from mwa-cli, or from an older giant-squid)
/// has empty values, which never match: see [`StoredTokens::belongs_to`].
#[derive(Serialize, Deserialize, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: Timestamp,
    pub refresh_expires_at: Timestamp,
    pub user_id: u64,
    pub user_login: String,
    pub user_email: String,
    /// The MWA ASVO host that issued the session, as in
    /// [`MwaAsvoClientConfig::host`](crate::MwaAsvoClientConfig::host).
    #[serde(default)]
    pub host: String,
    /// The SHA-256 of the API key that the session was made with, as
    /// lower-case hexadecimal. The key itself is never stored.
    #[serde(default)]
    pub api_key_sha256: String,
}

/// The tokens are not shown: they work as passwords.
impl std::fmt::Debug for StoredTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredTokens")
            .field("access_token", &crate::mwa_asvo::redact::REDACTED)
            .field("refresh_token", &crate::mwa_asvo::redact::REDACTED)
            .field("access_expires_at", &self.access_expires_at)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .field("user_id", &self.user_id)
            .field("user_login", &self.user_login)
            .field("user_email", &self.user_email)
            .field("host", &self.host)
            .field("api_key_sha256", &self.api_key_sha256)
            .finish()
    }
}

impl StoredTokens {
    /// Whether this session was made for `host` and `api_key`. A session of
    /// another host or another key, or a session in the old format (with no
    /// host and no fingerprint), is not.
    pub fn belongs_to(&self, host: &str, api_key: &str) -> bool {
        self.host == host && self.api_key_sha256 == api_key_sha256(api_key)
    }

    /// Is the access token still safe to use right now?
    pub fn is_access_valid(&self) -> bool {
        self.access_expires_at > Timestamp::now() + EXPIRY_SAFETY_BUFFER
    }

    /// Is the refresh token still safe to use right now?
    pub fn is_refresh_valid(&self) -> bool {
        self.refresh_expires_at > Timestamp::now() + EXPIRY_SAFETY_BUFFER
    }
}

/// The SHA-256 of `api_key`, as lower-case hexadecimal: what the cached
/// session holds in place of the key.
pub(crate) fn api_key_sha256(api_key: &str) -> String {
    to_hex(&Sha256::digest(api_key.as_bytes()))
}

/// Returns `<home>/.mwa-asvo/tokens.json`: the path of mwa-cli's token
/// cache, which giant-squid also uses. Pass the result as
/// [`MwaAsvoClientConfig::token_cache_path`](crate::MwaAsvoClientConfig::token_cache_path)
/// to cache the session there. A session that mwa-cli wrote is not used
/// (see [`StoredTokens::belongs_to`]).
pub fn default_token_cache_path(home: &Path) -> PathBuf {
    home.join(TOKEN_CACHE_DIR).join(TOKEN_CACHE_FILE)
}

/// Load cached tokens from disk, if any exist and are readable/parseable.
///
/// Any failure here (file missing, unreadable, corrupt JSON, wrong shape)
/// is treated as "no usable cached session" rather than a hard error, since
/// falling back to a fresh login is always a safe recovery path.
pub fn load(path: &Path) -> Option<StoredTokens> {
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let path_display = path.display();
            debug!("No usable cached MWA ASVO session at {path_display}: {e}");
            return None;
        }
    };

    match serde_json::from_str::<StoredTokens>(&contents) {
        Ok(tokens) => Some(tokens),
        Err(e) => {
            let path_display = path.display();
            debug!(
                "Cached MWA ASVO session at {path_display} could not be parsed, ignoring it: {e}"
            );
            None
        }
    }
}

/// Persist tokens to disk at `path`, creating the directory if needed (with
/// mode 0700 on Unix) and restricting the file to the current user (mode
/// 0600 from the moment it exists). The file is replaced in one step, so a
/// reader or a crash never sees a half-written file. A directory that is
/// there and is open to other users is not changed, but a warning says so.
///
/// This is best-effort: a failure to save is logged but is not treated as
/// fatal, since the JWT itself is still usable for the remainder of this
/// process even if we couldn't cache it for next time.
pub fn save(path: &Path, tokens: &StoredTokens) {
    if let Err(e) = save_inner(path, tokens) {
        let path_display = path.display();
        debug!("Could not cache MWA ASVO session to {path_display}: {e}");
    }
}

/// The work of [`save`]: write the JSON to a new private file in the same
/// directory, then rename it over `path`.
fn save_inner(path: &Path, tokens: &StoredTokens) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        prepare_directory(dir)?;
    }

    let json = serde_json::to_string_pretty(tokens)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let temporary = temporary_path(path);
    let result = write_private_file(&temporary, &json).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        // Best effort: the file may not exist, and the error that matters
        // is the one of the save.
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Make sure that the directory `dir` of the token cache exists. A directory
/// that is there is not changed, because the user (or mwa-cli) made it.
fn prepare_directory(dir: &Path) -> io::Result<()> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    if dir.is_dir() {
        #[cfg(unix)]
        warn_if_directory_is_open(dir);
        return Ok(());
    }

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(TOKEN_DIR_MODE);
    }
    builder.create(dir)
}

/// Log a warning if other users have any permission on the existing
/// directory `dir` of the token cache.
#[cfg(unix)]
fn warn_if_directory_is_open(dir: &Path) {
    if let Some(mode) = open_directory_mode(dir) {
        let shown = dir.display();
        log::warn!(
            "The directory {shown} of the cached MWA ASVO session is open to other users (mode {mode:03o}). \
             The session file in it is private (mode {TOKEN_FILE_MODE:03o}), but to close the directory too, \
             run: chmod {TOKEN_DIR_MODE:03o} {shown}"
        );
    }
}

/// The mode of the directory `dir`, if group or others have any permission
/// on it. `None` if only its owner has, or if its mode cannot be read.
#[cfg(unix)]
fn open_directory_mode(dir: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(dir).ok()?.permissions().mode() & PERMISSION_BITS;
    (mode & GROUP_AND_OTHER_BITS != 0).then_some(mode)
}

/// A name for the temporary file of a save of `path`, in the same
/// directory (a rename from another file system is not one step).
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| TOKEN_CACHE_FILE.into(), |n| n.to_string_lossy());
    let process = std::process::id();
    let count = TEMPORARY_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{process}.{count}{TEMPORARY_SUFFIX}"))
}

/// Create the new file `path` with [`TOKEN_FILE_MODE`] (Unix), write
/// `contents` and sync it. It fails if `path` is there already.
fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(TOKEN_FILE_MODE);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}
