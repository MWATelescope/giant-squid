// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Builds the library's [`AsvoClientConfig`] from the environment.
//!
//! The library reads no environment variables. The CLI does, here, and
//! gives the library an explicit config.

use std::env::var;
use std::path::Path;
use std::time::Duration;

use log::{debug, warn};

use crate::asvo::{
    default_token_cache_path, AsvoApiError, AsvoClientConfig, DEFAULT_API_TIMEOUT,
    DEFAULT_ASVO_HOST,
};

/// Overrides the MWA ASVO host (default [`DEFAULT_ASVO_HOST`]).
pub const ENV_MWA_ASVO_HOST: &str = "MWA_ASVO_HOST";
/// The user's MWA ASVO API key. Required.
pub const ENV_MWA_ASVO_API_KEY: &str = "MWA_ASVO_API_KEY";
/// Overrides the API request timeout, in whole seconds (default
/// [`DEFAULT_API_TIMEOUT`]).
pub const ENV_MWA_ASVO_API_TIMEOUT: &str = "MWA_ASVO_API_TIMEOUT";
/// The home directory. The token cache (shared with mwa-cli) is kept
/// under it. If it is not set, the session is not cached.
pub const ENV_HOME: &str = "HOME";

/// Build an [`AsvoClientConfig`] from the environment.
///
/// Returns [`AsvoApiError::MissingAuthKey`] if `MWA_ASVO_API_KEY` is not
/// set. An invalid `MWA_ASVO_API_TIMEOUT` is logged and the default is
/// used.
pub fn client_config_from_env() -> Result<AsvoClientConfig, AsvoApiError> {
    let api_key = var(ENV_MWA_ASVO_API_KEY).map_err(|_| AsvoApiError::MissingAuthKey)?;
    let host = var(ENV_MWA_ASVO_HOST).unwrap_or_else(|_| DEFAULT_ASVO_HOST.to_string());

    let mut config = AsvoClientConfig::new(host, api_key);
    config.api_timeout = api_timeout_from_env();
    config.token_cache_path = var(ENV_HOME)
        .ok()
        .map(|home| default_token_cache_path(Path::new(&home)));
    if config.token_cache_path.is_none() {
        debug!(
            "{} is not set; the MWA ASVO session will not be cached",
            ENV_HOME
        );
    }

    Ok(config)
}

/// The API timeout from `MWA_ASVO_API_TIMEOUT`, or the default if it is
/// not set or not a whole number of seconds.
fn api_timeout_from_env() -> Duration {
    let Ok(val) = var(ENV_MWA_ASVO_API_TIMEOUT) else {
        // Env variable was not present, no worries
        return DEFAULT_API_TIMEOUT;
    };

    match val.parse::<u64>() {
        Ok(num) => {
            debug!(
                "{} timeout overidden to {} seconds",
                ENV_MWA_ASVO_API_TIMEOUT, num
            );
            Duration::from_secs(num)
        }
        Err(e) => {
            warn!(
                "Environment variable {}='{}' is not valid, defaulting to {}. (It should be an integer number of seconds). Error: {}",
                ENV_MWA_ASVO_API_TIMEOUT,
                val,
                DEFAULT_API_TIMEOUT.as_secs(),
                e
            );
            DEFAULT_API_TIMEOUT
        }
    }
}
