//! The redaction of URLs and bodies for a log.

use super::{redact_body, redact_url, redact_urls_in_text};

/// A signature, as long as a real one, which no output may contain.
const SIGNATURE: &str = "aB3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9f";

/// An access key ID of a signed URL, which is long enough to show a part.
const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";

fn signed_url() -> String {
    format!(
        "https://projects.example.org:8443/bucket/1234_5678.tar\
         ?AWSAccessKeyId={ACCESS_KEY}&Expires=1790000000&Signature={SIGNATURE}"
    )
}

#[test]
fn a_signed_url_keeps_its_host_path_and_parameter_names() {
    let redacted = redact_url(&signed_url());

    assert!(
        redacted.starts_with("https://projects.example.org:8443/bucket/1234_5678.tar?"),
        "redacted: {redacted}"
    );
    for name in ["AWSAccessKeyId=", "Expires=", "Signature="] {
        assert!(redacted.contains(name), "redacted: {redacted}");
    }
}

#[test]
fn a_long_query_value_shows_only_its_first_characters() {
    let redacted = redact_url(&signed_url());

    assert!(
        redacted.contains("Signature=aB3d..."),
        "redacted: {redacted}"
    );
    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
    assert!(!redacted.contains(ACCESS_KEY), "redacted: {redacted}");
}

#[test]
fn a_short_query_value_is_replaced_whole() {
    let redacted = redact_url("https://example.org/x.tar?token=short-secret");

    assert_eq!(redacted, "https://example.org/x.tar?token=<redacted>");
}

#[test]
fn an_empty_query_value_stays_empty() {
    let redacted = redact_url("https://example.org/x.tar?flag=&other=1");

    assert_eq!(redacted, "https://example.org/x.tar?flag=&other=<redacted>");
}

#[test]
fn a_url_without_a_query_is_unchanged() {
    let redacted = redact_url("https://asvo.example.org/api/v2/jobs/list");

    assert_eq!(redacted, "https://asvo.example.org/api/v2/jobs/list");
}

#[test]
fn a_user_name_a_password_and_a_fragment_are_removed() {
    let redacted = redact_url("https://user:hunter2@example.org/x.tar#frag-secret");

    assert_eq!(redacted, "https://example.org/x.tar");
}

#[test]
fn a_text_that_is_not_a_url_is_replaced() {
    assert_eq!(redact_url("not a url, secret=1"), "<redacted url>");
}

#[test]
fn every_url_in_a_text_is_redacted_and_the_rest_is_kept() {
    let text = format!(
        "first {a} then \"{b}\" and <{a}> end",
        a = signed_url(),
        b = signed_url()
    );

    let redacted = redact_urls_in_text(&text);

    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
    assert!(redacted.starts_with("first https://projects.example.org:8443/"));
    assert!(redacted.contains(" then \"https://"));
    assert!(redacted.ends_with("> end"), "redacted: {redacted}");
    assert_eq!(redacted.matches("Signature=aB3d...").count(), 3);
}

#[test]
fn a_text_without_a_url_is_unchanged() {
    let text = "Skipping a job: no obs_id (state: completed)";

    assert_eq!(redact_urls_in_text(text), text);
}

#[test]
fn a_url_at_the_end_of_a_text_is_redacted() {
    let redacted = redact_urls_in_text(&format!("see {}", signed_url()));

    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
}

#[test]
fn the_urls_in_the_strings_of_a_json_body_are_redacted() {
    let body = format!(
        r#"{{"jobs":[{{"id":1,"product":{{"files":[{{"url":"{}","size":3}}]}}}}],"total_count":1}}"#,
        signed_url()
    );

    let redacted = redact_body(&body);

    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
    assert!(!redacted.contains(ACCESS_KEY), "redacted: {redacted}");
    assert!(
        redacted.contains("\"total_count\":1"),
        "redacted: {redacted}"
    );
    assert!(
        redacted.contains("/bucket/1234_5678.tar"),
        "redacted: {redacted}"
    );
}

/// A server can escape `&` as `\u0026`. The JSON is parsed before the
/// redaction, so the escape does not end the URL early.
#[test]
fn an_escaped_ampersand_does_not_hide_the_end_of_a_url() {
    let body = format!(
        r#"{{"url":"https://example.org/x.tar?AWSAccessKeyId={ACCESS_KEY}\u0026Signature={SIGNATURE}"}}"#
    );

    let redacted = redact_body(&body);

    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
    assert!(!redacted.contains(ACCESS_KEY), "redacted: {redacted}");
}

#[test]
fn the_urls_in_a_body_that_is_not_json_are_redacted() {
    let body = format!("<html>Forbidden: {}</html>", signed_url());

    let redacted = redact_body(&body);

    assert!(!redacted.contains(SIGNATURE), "redacted: {redacted}");
    assert!(redacted.starts_with("<html>Forbidden: https://"));
}
