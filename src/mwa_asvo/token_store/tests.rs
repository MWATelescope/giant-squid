// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests for the token cache, which is shared with mwa-cli.
//!
//! The file format must not change when the date and time library does:
//! mwa-cli and older giant-squid versions read and write the same file.

use super::*;

/// A `tokens.json` with the given expiry text in both expiry fields.
fn tokens_json(expiry: &str) -> String {
    format!(
        r#"{{"access_token": "a", "refresh_token": "r",
             "access_expires_at": "{expiry}", "refresh_expires_at": "{expiry}",
             "user_id": 4242, "user_login": "test_user", "user_email": "test@example.org"}}"#
    )
}

/// Every form of UTC time that the file may hold is read as the same
/// instant: `Z` (giant-squid 3.0.0 before jiff wrote this, and so does
/// jiff), a `+00:00` offset (Python's `isoformat()`), and fractional
/// seconds.
#[test]
fn every_utc_form_in_the_file_is_read() {
    let expected: Timestamp = "2026-09-30T06:00:00Z".parse().expect("a valid time");

    for (text, instant) in [
        ("2026-09-30T06:00:00Z", expected),
        ("2026-09-30T06:00:00+00:00", expected),
        ("2026-09-30T08:00:00+02:00", expected),
        (
            "2026-09-30T06:00:00.123456+00:00",
            "2026-09-30T06:00:00.123456Z".parse().expect("a valid time"),
        ),
    ] {
        let tokens: StoredTokens =
            serde_json::from_str(&tokens_json(text)).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(tokens.access_expires_at, instant, "{text}");
        assert_eq!(tokens.refresh_expires_at, instant, "{text}");
    }
}

/// The file is written as RFC 3339 in UTC with `Z`, as chrono wrote it, so
/// an older giant-squid or mwa-cli still reads it.
#[test]
fn the_file_is_written_as_rfc3339_utc() {
    let tokens: StoredTokens = serde_json::from_str(&tokens_json("2026-09-30T08:00:00+02:00"))
        .expect("a valid token file");

    let written = serde_json::to_value(&tokens).expect("the tokens serialise");

    assert_eq!(written["access_expires_at"], "2026-09-30T06:00:00Z");
    assert_eq!(written["refresh_expires_at"], "2026-09-30T06:00:00Z");
}

/// A time with no offset is ambiguous, and was refused before jiff too.
#[test]
fn a_time_without_an_offset_is_refused() {
    assert!(serde_json::from_str::<StoredTokens>(&tokens_json("2026-09-30T06:00:00")).is_err());
}

// ---------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------

/// Tokens to save.
fn sample_tokens() -> StoredTokens {
    serde_json::from_str(&tokens_json("2026-09-30T06:00:00Z")).expect("valid tokens")
}

/// The permission bits of `path`.
#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .expect("the path exists")
        .permissions()
        .mode()
        & PERMISSION_BITS
}

#[test]
fn a_saved_session_is_loaded_again_and_leaves_no_temporary_file() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let path = dir.path().join(".mwa-asvo").join(TOKEN_CACHE_FILE);
    let tokens = sample_tokens();

    save(&path, &tokens);
    save(&path, &tokens);

    let loaded = load(&path).expect("the saved session loads");
    assert_eq!(loaded.access_token, tokens.access_token);
    let names: Vec<_> = fs::read_dir(path.parent().expect("a parent"))
        .expect("the directory is there")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(names, [TOKEN_CACHE_FILE]);
}

#[cfg(unix)]
#[test]
fn a_new_directory_is_private_and_so_is_the_file() {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let path = dir.path().join(".mwa-asvo").join(TOKEN_CACHE_FILE);

    save(&path, &sample_tokens());

    assert_eq!(mode_of(path.parent().expect("a parent")), TOKEN_DIR_MODE);
    assert_eq!(mode_of(&path), TOKEN_FILE_MODE);
}

/// A file that is there with open permissions is replaced by a private one.
#[cfg(unix)]
#[test]
fn a_file_with_open_permissions_is_replaced_by_a_private_one() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let path = dir.path().join(TOKEN_CACHE_FILE);
    fs::write(&path, "old").expect("write the old file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("open the file");

    save(&path, &sample_tokens());

    assert_eq!(mode_of(&path), TOKEN_FILE_MODE);
    assert!(load(&path).is_some());
}

/// A directory that is there is not changed, even if it is open.
#[cfg(unix)]
#[test]
fn an_existing_directory_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).expect("open the directory");

    save(&dir.path().join(TOKEN_CACHE_FILE), &sample_tokens());

    assert_eq!(mode_of(dir.path()), 0o755);
}

/// The warning is for a directory on which group or others have any
/// permission; the mode of the owner does not matter.
#[cfg(unix)]
#[test]
fn a_directory_is_open_when_group_or_others_have_a_permission() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().expect("a temporary directory");

    for (mode, open) in [
        (0o700, false),
        (0o500, false),
        (0o750, true),
        (0o710, true),
        (0o705, true),
        (0o755, true),
        (0o777, true),
    ] {
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(mode)).expect("set the mode");
        let found = open_directory_mode(dir.path());
        assert_eq!(found.is_some(), open, "mode {mode:03o}");
        if open {
            assert_eq!(found, Some(mode));
        }
    }
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700))
        .expect("close the directory");
}

// ---------------------------------------------------------------------------
// The owner of a session
// ---------------------------------------------------------------------------

const HOST: &str = "https://asvo.example.org";
const KEY: &str = "the-api-key";

/// Tokens for `HOST` and `KEY`.
fn owned_tokens() -> StoredTokens {
    StoredTokens {
        host: HOST.to_string(),
        api_key_sha256: api_key_sha256(KEY),
        ..sample_tokens()
    }
}

/// The SHA-256 test vector for "abc" (FIPS 180-2), in lower-case hex.
#[test]
fn the_key_hash_is_the_sha256_in_lower_case_hex() {
    assert_eq!(
        api_key_sha256("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn a_session_belongs_to_its_host_and_key_only() {
    let tokens = owned_tokens();

    assert!(tokens.belongs_to(HOST, KEY));
    assert!(!tokens.belongs_to("https://another.example.org", KEY));
    assert!(!tokens.belongs_to(HOST, "another-key"));
    assert!(!tokens.belongs_to(HOST, ""));
}

/// A file from mwa-cli or an older giant-squid has neither field: it loads,
/// and it belongs to nobody, not even a client with an empty host.
#[test]
fn a_session_in_the_old_format_belongs_to_nobody() {
    let old: StoredTokens =
        serde_json::from_str(&tokens_json("2026-09-30T06:00:00Z")).expect("an old file loads");

    assert_eq!(old.host, "");
    assert_eq!(old.api_key_sha256, "");
    assert!(!old.belongs_to(HOST, KEY));
    assert!(!old.belongs_to("", ""));
}

#[test]
fn the_file_holds_the_hash_of_the_key_and_never_the_key() {
    let written = serde_json::to_string(&owned_tokens()).expect("the tokens serialise");

    assert!(written.contains(&api_key_sha256(KEY)));
    assert!(!written.contains(KEY));
    let back: StoredTokens = serde_json::from_str(&written).expect("the file loads again");
    assert!(back.belongs_to(HOST, KEY));
}
