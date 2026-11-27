// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Settings from the environment, for a program that wants the settings of
//! the `giant-squid` command.
//!
//! The rest of the library reads no environment variable. A program calls
//! [`client_config_from_env`] or [`DownloadSettings::from_env`] to get an
//! [`MwaAsvoClientConfig`] or download settings, or builds its own. The
//! messages about the variables are here, so that every program that reads
//! them says the same thing.

use std::path::Path;
use std::time::Duration;

use log::debug;

use crate::mwa_asvo::api::openapi::{DeliveryDestination, DeliveryFormat};
use crate::mwa_asvo::api::schema_enums::SchemaEnum;
use crate::mwa_asvo::manta_ray_csv::CsvDefaults;
use crate::mwa_asvo::{
    default_token_cache_path, MwaAsvoApiError, MwaAsvoClientConfig, MwaAsvoError, BYTES_PER_MIB,
    DEFAULT_API_TIMEOUT, DEFAULT_DOWNLOAD_BUFFER_SIZE, DEFAULT_DOWNLOAD_RETRY_DURATION,
    DEFAULT_MWA_ASVO_HOST,
};

#[cfg(test)]
mod tests;

/// Overrides the MWA ASVO host (default [`DEFAULT_MWA_ASVO_HOST`]).
pub const ENV_MWA_ASVO_HOST: &str = "MWA_ASVO_HOST";
/// The user's MWA ASVO API key. Required.
pub const ENV_MWA_ASVO_API_KEY: &str = "MWA_ASVO_API_KEY";
/// Overrides the API request timeout, in whole seconds (default
/// [`DEFAULT_API_TIMEOUT`]).
pub const ENV_MWA_ASVO_API_TIMEOUT: &str = "MWA_ASVO_API_TIMEOUT";
/// The home directory. The token cache is kept under it, at the path that
/// mwa-cli also uses. If it is not set, the session is not cached.
pub const ENV_HOME: &str = "HOME";
/// Overrides the download buffer size, in whole MiB (default
/// [`DEFAULT_DOWNLOAD_BUFFER_SIZE`]).
pub const ENV_GIANT_SQUID_BUF_SIZE: &str = "GIANT_SQUID_BUF_SIZE";
/// Overrides how long a download retries transient failures, in whole
/// seconds (default [`DEFAULT_DOWNLOAD_RETRY_DURATION`]). Zero disables
/// retrying.
pub const ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS: &str = "GIANT_SQUID_DOWNLOAD_RETRY_SECS";
/// The default of `--delivery` of the submit commands.
pub const ENV_GIANT_SQUID_DELIVERY: &str = "GIANT_SQUID_DELIVERY";
/// The default of `--delivery-format` of the submit commands.
pub const ENV_GIANT_SQUID_DELIVERY_FORMAT: &str = "GIANT_SQUID_DELIVERY_FORMAT";

/// What a message says a whole-number variable should be.
const EXPECTED_SECONDS: &str = "an integer number of seconds";
const EXPECTED_MIB: &str = "an integer number of MiB";

/// The value of a variable of the process environment, or `None` if it is not
/// set (or is not text).
fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Build an [`MwaAsvoClientConfig`] from the environment.
///
/// - `MWA_ASVO_API_KEY` is required.
/// - `MWA_ASVO_HOST` defaults to [`DEFAULT_MWA_ASVO_HOST`].
/// - `MWA_ASVO_API_TIMEOUT` is a whole number of seconds.
/// - The session is cached under `HOME` (see [`default_token_cache_path`]).
///   Without `HOME` it is not cached.
///
/// # Errors
///
/// [`MwaAsvoError::Api`] with [`MwaAsvoApiError::MissingAuthKey`] if
/// `MWA_ASVO_API_KEY` is not set, and [`MwaAsvoError::InvalidEnvironment`] if
/// `MWA_ASVO_API_TIMEOUT` is set but is not a whole number of seconds.
pub fn client_config_from_env() -> Result<MwaAsvoClientConfig, MwaAsvoError> {
    client_config_from(process_env)
}

/// As [`client_config_from_env`], reading the variables with `get`.
fn client_config_from(
    get: impl Fn(&str) -> Option<String>,
) -> Result<MwaAsvoClientConfig, MwaAsvoError> {
    let api_key = get(ENV_MWA_ASVO_API_KEY).ok_or(MwaAsvoApiError::MissingAuthKey {
        variable: Some(ENV_MWA_ASVO_API_KEY),
    })?;
    let host = get(ENV_MWA_ASVO_HOST).unwrap_or_else(|| DEFAULT_MWA_ASVO_HOST.to_string());

    let mut config = MwaAsvoClientConfig::new(host, api_key);
    config.api_timeout = match get(ENV_MWA_ASVO_API_TIMEOUT) {
        None => DEFAULT_API_TIMEOUT,
        Some(value) => {
            let seconds: u64 = whole_number(ENV_MWA_ASVO_API_TIMEOUT, &value, EXPECTED_SECONDS)?;
            if seconds == 0 {
                return Err(MwaAsvoError::InvalidEnvironment {
                    name: ENV_MWA_ASVO_API_TIMEOUT.to_string(),
                    value,
                    problem: format!("is not valid. (It should be {EXPECTED_SECONDS}, at least 1)"),
                });
            }
            debug!("{ENV_MWA_ASVO_API_TIMEOUT} timeout overridden to {seconds} seconds");
            Duration::from_secs(seconds)
        }
    };
    config.token_cache_path = get(ENV_HOME).map(|home| default_token_cache_path(Path::new(&home)));
    if config.token_cache_path.is_none() {
        debug!("{ENV_HOME} is not set; the MWA ASVO session will not be cached");
    }

    Ok(config)
}

/// The value `value` of the variable `name` as a whole number.
///
/// # Errors
///
/// [`MwaAsvoError::InvalidEnvironment`] if it is not one; the message says it
/// should be `expected`.
fn whole_number<T: std::str::FromStr>(
    name: &str,
    value: &str,
    expected: &str,
) -> Result<T, MwaAsvoError> {
    value.parse().map_err(|_| MwaAsvoError::InvalidEnvironment {
        name: name.to_string(),
        value: value.to_string(),
        problem: format!("is not valid. (It should be {expected})"),
    })
}

/// The download settings that come from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadSettings {
    /// How many bytes to hold in memory before they are written
    /// ([`DownloadOptions::buffer_size`](crate::DownloadOptions::buffer_size)).
    pub buffer_size: usize,
    /// How long to retry a failing download
    /// ([`DownloadOptions::retry_duration`](crate::DownloadOptions::retry_duration)).
    pub retry_duration: Duration,
}

impl DownloadSettings {
    /// Read the download settings from the environment.
    ///
    /// - `GIANT_SQUID_BUF_SIZE` is a whole number of MiB (default
    ///   [`DEFAULT_DOWNLOAD_BUFFER_SIZE`]).
    /// - `GIANT_SQUID_DOWNLOAD_RETRY_SECS` is a whole number of seconds
    ///   (default [`DEFAULT_DOWNLOAD_RETRY_DURATION`]).
    ///
    /// # Errors
    ///
    /// [`MwaAsvoError::InvalidEnvironment`] if `GIANT_SQUID_BUF_SIZE` is set but
    /// is not a whole number of MiB, or is too large, or if
    /// `GIANT_SQUID_DOWNLOAD_RETRY_SECS` is set but is not a whole number of
    /// seconds.
    pub fn from_env() -> Result<Self, MwaAsvoError> {
        Self::from(process_env)
    }

    /// As [`Self::from_env`], reading the variables with `get`.
    fn from(get: impl Fn(&str) -> Option<String>) -> Result<Self, MwaAsvoError> {
        let buffer_size = match get(ENV_GIANT_SQUID_BUF_SIZE) {
            None => DEFAULT_DOWNLOAD_BUFFER_SIZE,
            Some(value) => {
                let mib: usize = whole_number(ENV_GIANT_SQUID_BUF_SIZE, &value, EXPECTED_MIB)?;
                if mib == 0 {
                    return Err(MwaAsvoError::InvalidEnvironment {
                        name: ENV_GIANT_SQUID_BUF_SIZE.to_string(),
                        value,
                        problem: format!("is not valid. (It should be {EXPECTED_MIB}, at least 1)"),
                    });
                }
                mib.checked_mul(BYTES_PER_MIB)
                    .ok_or_else(|| MwaAsvoError::InvalidEnvironment {
                        name: ENV_GIANT_SQUID_BUF_SIZE.to_string(),
                        value: value.clone(),
                        problem: "is too large".to_string(),
                    })?
            }
        };

        let retry_duration = match get(ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS) {
            None => DEFAULT_DOWNLOAD_RETRY_DURATION,
            Some(value) => Duration::from_secs(whole_number(
                ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS,
                &value,
                EXPECTED_SECONDS,
            )?),
        };

        Ok(Self {
            buffer_size,
            retry_duration,
        })
    }
}

/// The value of the variable `name` as a value of the schema enum `T`, as
/// `--delivery` and the other options read it (the case, spaces, hyphens and
/// underscores do not matter), or `None` if the variable is not set.
///
/// # Errors
///
/// [`MwaAsvoError::InvalidEnvironment`] if it is set to something else.
fn named_value<T: SchemaEnum>(
    get: &impl Fn(&str) -> Option<String>,
    name: &str,
) -> Result<Option<T>, MwaAsvoError> {
    get(name)
        .map(|value| {
            T::from_name(&value).ok_or_else(|| MwaAsvoError::InvalidEnvironment {
                name: name.to_string(),
                problem: format!(
                    "is not valid. (It should be one of {})",
                    T::names().join(", ")
                ),
                value,
            })
        })
        .transpose()
}

impl CsvDefaults {
    /// The defaults of a `manta-ray-client` CSV file that come from the
    /// environment, as for the `submit-*` commands.
    ///
    /// - `GIANT_SQUID_DELIVERY` is the `delivery` of a line that has none.
    /// - `GIANT_SQUID_DELIVERY_FORMAT` is the `delivery_format` of a line
    ///   that has none.
    ///
    /// `allow_resubmit` is not read from the environment.
    ///
    /// # Errors
    ///
    /// [`MwaAsvoError::InvalidEnvironment`] if a variable is set to a value
    /// that the schema does not list.
    pub fn from_env() -> Result<Self, MwaAsvoError> {
        Self::from(process_env)
    }

    /// As [`Self::from_env`], reading the variables with `get`.
    fn from(get: impl Fn(&str) -> Option<String>) -> Result<Self, MwaAsvoError> {
        Ok(Self {
            delivery: named_value::<DeliveryDestination>(&get, ENV_GIANT_SQUID_DELIVERY)?,
            delivery_format: named_value::<DeliveryFormat>(&get, ENV_GIANT_SQUID_DELIVERY_FORMAT)?,
            allow_resubmit: None,
        })
    }
}
