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
