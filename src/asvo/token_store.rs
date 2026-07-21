// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Local storage for MWA ASVO JWT session tokens.
//!
//! This file is intentionally compatible with (and shared with) mwa-cli's
//! own token cache, so that logging in with either client makes a valid
//! session available to the other. The location and JSON shape here
//! (`$HOME/.mwa-asvo/tokens.json`) must stay in sync with mwa-cli's format.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};

/// How much of a safety buffer to apply when deciding if a token is still
/// valid. This avoids a token expiring mid-flight between the check and the
/// request actually being sent.
const EXPIRY_SAFETY_BUFFER: Duration = Duration::from_secs(30);

/// A cached MWA ASVO JWT session, as persisted to disk.
///
/// The field names and shape here must exactly match mwa-cli's own
/// `tokens.json`, since the file is shared between the two clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: DateTime<Utc>,
    pub refresh_expires_at: DateTime<Utc>,
    pub user_id: u64,
    pub user_login: String,
    pub user_email: String,
}

impl StoredTokens {
    /// Is the access token still safe to use right now?
    pub fn is_access_valid(&self) -> bool {
        self.access_expires_at
            > Utc::now() + chrono::Duration::from_std(EXPIRY_SAFETY_BUFFER).unwrap()
    }

    /// Is the refresh token still safe to use right now?
    pub fn is_refresh_valid(&self) -> bool {
        self.refresh_expires_at
            > Utc::now() + chrono::Duration::from_std(EXPIRY_SAFETY_BUFFER).unwrap()
    }
}

/// Returns `$HOME/.mwa-asvo/tokens.json`, or `None` if `HOME` isn't set.
///
/// A missing `HOME` is not an error condition for callers: it just means
/// session caching isn't available, and every command should fall back to
/// a fresh login.
fn token_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".mwa-asvo").join("tokens.json"))
}

/// Load cached tokens from disk, if any exist and are readable/parseable.
///
/// Any failure here (file missing, unreadable, corrupt JSON, wrong shape)
/// is treated as "no usable cached session" rather than a hard error, since
/// falling back to a fresh login is always a safe recovery path.
pub fn load() -> Option<StoredTokens> {
    let path = token_path()?;

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                "No usable cached MWA ASVO session at {}: {}",
                path.display(),
                e
            );
            return None;
        }
    };

    match serde_json::from_str::<StoredTokens>(&contents) {
        Ok(tokens) => Some(tokens),
        Err(e) => {
            debug!(
                "Cached MWA ASVO session at {} could not be parsed, ignoring it: {}",
                path.display(),
                e
            );
            None
        }
    }
}

/// Persist tokens to disk at `$HOME/.mwa-asvo/tokens.json`, creating the
/// directory if needed and restricting permissions to the current user.
///
/// This is best-effort: a failure to save is logged but is not treated as
/// fatal, since the JWT itself is still usable for the remainder of this
/// process even if we couldn't cache it for next time.
pub fn save(tokens: &StoredTokens) {
    let Some(path) = token_path() else {
        debug!("HOME is not set; skipping caching of MWA ASVO session");
        return;
    };

    if let Err(e) = save_inner(&path, tokens) {
        debug!(
            "Could not cache MWA ASVO session to {}: {}",
            path.display(),
            e
        );
    }
}

fn save_inner(path: &PathBuf, tokens: &StoredTokens) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(tokens)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    fs::write(path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}
