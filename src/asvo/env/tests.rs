// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tests of the settings from the environment. They read the variables
//! through a lookup function, so no test touches the process environment and
//! the tests can run in parallel.

use std::collections::HashMap;
use std::path::PathBuf;

use super::*;

/// A lookup function over the given variables.
fn vars<const N: usize>(pairs: [(&str, &str); N]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

/// A key that no test checks the value of.
const KEY: &str = "my-api-key";

#[test]
fn the_names_are_the_ones_the_documentation_gives() {
    assert_eq!(ENV_MWA_ASVO_API_KEY, "MWA_ASVO_API_KEY");
    assert_eq!(ENV_MWA_ASVO_HOST, "MWA_ASVO_HOST");
    assert_eq!(ENV_MWA_ASVO_API_TIMEOUT, "MWA_ASVO_API_TIMEOUT");
    assert_eq!(ENV_HOME, "HOME");
    assert_eq!(ENV_GIANT_SQUID_BUF_SIZE, "GIANT_SQUID_BUF_SIZE");
    assert_eq!(
        ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS,
        "GIANT_SQUID_DOWNLOAD_RETRY_SECS"
    );
    assert_eq!(ENV_GIANT_SQUID_DELIVERY, "GIANT_SQUID_DELIVERY");
    assert_eq!(
        ENV_GIANT_SQUID_DELIVERY_FORMAT,
        "GIANT_SQUID_DELIVERY_FORMAT"
    );
}

#[test]
fn a_missing_api_key_is_an_error() {
    let err = client_config_from(vars([])).expect_err("no key");

    assert!(matches!(err, AsvoApiError::MissingAuthKey), "{err:?}");
}

#[test]
fn the_defaults_are_used_when_only_the_key_is_set() {
    let config = client_config_from(vars([(ENV_MWA_ASVO_API_KEY, KEY)])).expect("a config");

    assert_eq!(config.api_key, KEY);
    assert_eq!(config.host, DEFAULT_ASVO_HOST);
    assert_eq!(config.api_timeout, DEFAULT_API_TIMEOUT);
    assert_eq!(config.token_cache_path, None);
}

#[test]
fn the_host_the_timeout_and_the_home_are_read() {
    let config = client_config_from(vars([
        (ENV_MWA_ASVO_API_KEY, KEY),
        (ENV_MWA_ASVO_HOST, "http://localhost:1234"),
        (ENV_MWA_ASVO_API_TIMEOUT, "7"),
        (ENV_HOME, "/home/me"),
    ]))
    .expect("a config");

    assert_eq!(config.host, "http://localhost:1234");
    assert_eq!(config.api_timeout, Duration::from_secs(7));
    assert_eq!(
        config.token_cache_path,
        Some(default_token_cache_path(&PathBuf::from("/home/me")))
    );
}

/// A timeout that is not a whole number of seconds is not an error: the
/// default is used (and a warning is logged).
#[test]
fn a_bad_timeout_gives_the_default() {
    for bad in ["soon", "-1", "1.5", ""] {
        let config = client_config_from(vars([
            (ENV_MWA_ASVO_API_KEY, KEY),
            (ENV_MWA_ASVO_API_TIMEOUT, bad),
        ]))
        .expect("a config");

        assert_eq!(config.api_timeout, DEFAULT_API_TIMEOUT, "{bad:?}");
    }
}

#[test]
fn the_download_defaults_are_used_when_nothing_is_set() {
    let settings = DownloadSettings::from(vars([])).expect("settings");

    assert_eq!(settings.buffer_size, DEFAULT_DOWNLOAD_BUFFER_SIZE);
    assert_eq!(settings.retry_duration, DEFAULT_DOWNLOAD_RETRY_DURATION);
}

#[test]
fn the_buffer_size_is_in_mib_and_the_retry_in_seconds() {
    let settings = DownloadSettings::from(vars([
        (ENV_GIANT_SQUID_BUF_SIZE, "3"),
        (ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, "0"),
    ]))
    .expect("settings");

    assert_eq!(settings.buffer_size, 3 * BYTES_PER_MIB);
    assert_eq!(settings.retry_duration, Duration::ZERO);
}

/// One phrasing for the Rust and the Python command: this is the message.
#[test]
fn a_bad_buffer_size_is_an_error_with_one_message() {
    for bad in ["lots", "-1", "1.5", ""] {
        let err = DownloadSettings::from(vars([(ENV_GIANT_SQUID_BUF_SIZE, bad)]))
            .expect_err("a bad buffer size");

        assert_eq!(
            err.to_string(),
            format!(
                "Environment variable GIANT_SQUID_BUF_SIZE='{bad}' is not valid. \
                 (It should be an integer number of MiB)"
            )
        );
    }
}

#[test]
fn a_buffer_size_too_big_for_memory_is_an_error() {
    let value = usize::MAX.to_string();
    let err =
        DownloadSettings::from(vars([(ENV_GIANT_SQUID_BUF_SIZE, &value)])).expect_err("too large");

    assert_eq!(
        err.to_string(),
        format!("Environment variable GIANT_SQUID_BUF_SIZE='{value}' is too large")
    );
}

/// A retry duration that is not a whole number of seconds is not an error:
/// the default is used (and a warning is logged).
#[test]
fn a_bad_retry_duration_gives_the_default() {
    for bad in ["forever", "-5", "1.5"] {
        let settings = DownloadSettings::from(vars([(ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, bad)]))
            .expect("settings");

        assert_eq!(
            settings.retry_duration, DEFAULT_DOWNLOAD_RETRY_DURATION,
            "{bad:?}"
        );
    }
}
