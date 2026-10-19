// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Redaction of secrets before they reach a log.
//!
//! A signed Acacia URL carries its credential in the query string, and an
//! HTTP body can carry a signed URL. The functions here make the text that
//! is safe to log. They keep what a person needs to find a problem (the
//! host, the path, the names of the query parameters, the first characters
//! of a long value) and remove the rest.
//!
//! The login and refresh bodies hold the API key and the tokens. They are
//! not redacted here: the client does not log them at all.

use std::fmt::Write as _;

use reqwest::Url;

#[cfg(test)]
mod tests;

/// The text that replaces a query value that the log must not show.
const REDACTED: &str = "<redacted>";

/// The text that replaces a URL that cannot be parsed, and so cannot be
/// redacted in parts.
const REDACTED_URL: &str = "<redacted url>";

/// The text that stands for the part of a query value that is not shown.
const ELLIPSIS: &str = "...";

/// How many characters of a long query value stay visible. They are enough
/// to tell two signed URLs apart, and too few to use one.
const VISIBLE_VALUE_CHARS: usize = 4;

/// A query value must be at least this long to show any of its characters.
/// A shorter value is replaced whole, because showing the first characters
/// of a short value would show most of it.
const MIN_LEN_TO_SHOW_PART: usize = 16;

/// The prefixes that start a URL in a piece of text.
const URL_PREFIXES: [&str; 2] = ["https://", "http://"];

/// The characters that end a URL in a piece of text.
const URL_END_CHARS: [char; 5] = ['"', '\'', '<', '>', ' '];

/// `url` without its secrets, for a log.
///
/// What stays: the scheme, the host, the port, the path, and the name of
/// each query parameter. What goes: a user name or password, the fragment,
/// and the value of each query parameter, except the first
/// [`VISIBLE_VALUE_CHARS`] characters of a value of at least
/// [`MIN_LEN_TO_SHOW_PART`] characters. A text that is not a URL gives
/// [`REDACTED_URL`].
pub(crate) fn redact_url(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) => redact_parsed_url(&parsed),
        Err(_) => REDACTED_URL.to_string(),
    }
}

/// As [`redact_url`], for a URL that is already parsed.
pub(crate) fn redact_parsed_url(url: &Url) -> String {
    let mut out = format!("{}://", url.scheme());
    if let Some(host) = url.host_str() {
        out.push_str(host);
    }
    if let Some(port) = url.port() {
        // `write!` to a `String` cannot fail.
        let _ = write!(out, ":{port}");
    }
    out.push_str(url.path());
    for (index, (name, value)) in url.query_pairs().enumerate() {
        out.push(if index == 0 { '?' } else { '&' });
        out.push_str(&name);
        out.push('=');
        out.push_str(&redact_query_value(&value));
    }
    out
}

/// A query value for a log: empty if it is empty, the first
/// [`VISIBLE_VALUE_CHARS`] characters and [`ELLIPSIS`] if it is long, and
/// [`REDACTED`] otherwise.
fn redact_query_value(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else if value.chars().count() >= MIN_LEN_TO_SHOW_PART {
        let shown: String = value.chars().take(VISIBLE_VALUE_CHARS).collect();
        format!("{shown}{ELLIPSIS}")
    } else {
        REDACTED.to_string()
    }
}

/// `text` with every URL in it redacted as [`redact_url`] does. A URL in
/// the text starts with `http://` or `https://` and ends at a quote, an
/// angle bracket or a space.
pub(crate) fn redact_urls_in_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = next_url_start(rest) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail.find(URL_END_CHARS).unwrap_or(tail.len());
        out.push_str(&redact_url(&tail[..end]));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Where the first URL in `text` starts.
fn next_url_start(text: &str) -> Option<usize> {
    URL_PREFIXES
        .iter()
        .filter_map(|prefix| text.find(prefix))
        .min()
}

/// The body of an HTTP response for a log, with its URLs redacted.
///
/// A body that is JSON is parsed first, and the URLs in its strings are
/// redacted. This way an escape such as `\u0026` in a URL cannot hide the
/// rest of the URL from the redaction. Any other body has the URLs in its
/// text redacted.
pub(crate) fn redact_body(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut value) => {
            redact_json_urls(&mut value);
            value.to_string()
        }
        Err(_) => redact_urls_in_text(body),
    }
}

/// Redact the URLs in every string of `value`.
fn redact_json_urls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => {
            if URL_PREFIXES.iter().any(|prefix| text.contains(prefix)) {
                *text = redact_urls_in_text(text);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact_json_urls),
        serde_json::Value::Object(map) => map.values_mut().for_each(redact_json_urls),
        _ => {}
    }
}
