// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The settings of a client and of a download: `AsvoClientConfig` and
//! `DownloadSettings`, and the functions that read them from the environment.
//!
//! A `Duration` of the library is a `float` number of seconds.

use std::path::PathBuf;
use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::error::asvo_error;
use crate::mwa_asvo::AsvoClientConfig;
use crate::{
    DownloadSettings, DEFAULT_API_TIMEOUT, DEFAULT_DOWNLOAD_BUFFER_SIZE,
    DEFAULT_DOWNLOAD_RETRY_DURATION,
};

/// Read a number of seconds as a `Duration`.
///
/// # Errors
///
/// `ValueError` if `seconds` is negative, not finite, or too large.
pub(super) fn duration_from_seconds(name: &str, seconds: f64) -> PyResult<Duration> {
    Duration::try_from_secs_f64(seconds)
        .map_err(|e| PyValueError::new_err(format!("{name}={seconds} is not valid: {e}")))
}

/// Everything an `AsvoClient` needs to connect to the MWA ASVO.
///
/// The module reads no environment variables here. To read them (as the
/// `giant-squid` command does), use `client_config_from_env`.
///
/// Args:
///     host: The MWA ASVO base URL, for example
///         "https://asvo.mwatelescope.org:443". An "http://" host is
///         permitted (a local test server); every other host must use TLS.
///     api_key: Your MWA ASVO API key.
///     api_timeout: The timeout for one API request, in seconds. More than 0.
///         `None` is `DEFAULT_API_TIMEOUT`.
///     token_cache_path: Where to cache the session between runs. `None`
///         keeps the session in memory only, so every new client logs in.
///         Give a path for a script that runs often, because the server
///         permits only a few logins a minute. A cached session is used only
///         by a client with the same `host` and `api_key`; the file holds a
///         hash of the key, never the key. `default_token_cache_path` is the
///         place that the `giant-squid` command uses.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "AsvoClientConfig", module = "mwa_giant_squid")]
pub struct PyAsvoClientConfig {
    /// The MWA ASVO base URL.
    #[pyo3(get, set)]
    pub host: String,
    /// Your MWA ASVO API key.
    #[pyo3(get, set)]
    pub api_key: String,
    /// The timeout for one API request, in seconds. More than 0.
    #[pyo3(get, set)]
    pub api_timeout: f64,
    /// Where to cache the session between runs, or `None`.
    #[pyo3(get, set)]
    pub token_cache_path: Option<PathBuf>,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyAsvoClientConfig {
    #[new]
    #[pyo3(signature = (host, api_key, *, api_timeout=None, token_cache_path=None))]
    fn new(
        host: String,
        api_key: String,
        api_timeout: Option<f64>,
        token_cache_path: Option<PathBuf>,
    ) -> Self {
        Self {
            host,
            api_key,
            api_timeout: api_timeout.unwrap_or_else(|| DEFAULT_API_TIMEOUT.as_secs_f64()),
            token_cache_path,
        }
    }

    /// The repr does not show the API key.
    fn __repr__(&self) -> String {
        format!(
            "AsvoClientConfig(host={:?}, api_key=<hidden>, api_timeout={}, token_cache_path={:?})",
            self.host, self.api_timeout, self.token_cache_path
        )
    }
}

impl PyAsvoClientConfig {
    /// The library's config for these values.
    ///
    /// # Errors
    ///
    /// `ValueError` if `api_timeout` is negative or not finite. (A timeout of
    /// 0 is refused when the client is made.)
    pub(super) fn to_lib(&self) -> PyResult<AsvoClientConfig> {
        let mut config = AsvoClientConfig::new(self.host.clone(), self.api_key.clone());
        config.api_timeout = duration_from_seconds("api_timeout", self.api_timeout)?;
        config.token_cache_path = self.token_cache_path.clone();
        Ok(config)
    }
}

impl From<AsvoClientConfig> for PyAsvoClientConfig {
    fn from(config: AsvoClientConfig) -> Self {
        Self {
            host: config.host,
            api_key: config.api_key,
            api_timeout: config.api_timeout.as_secs_f64(),
            token_cache_path: config.token_cache_path,
        }
    }
}

/// Build an `AsvoClientConfig` from the environment, as the `giant-squid`
/// command does.
///
/// - `MWA_ASVO_API_KEY` is required.
/// - `MWA_ASVO_HOST` is the host. The default is `DEFAULT_ASVO_HOST`.
/// - `MWA_ASVO_API_TIMEOUT` is a whole number of seconds. The default is
///   `DEFAULT_API_TIMEOUT`.
/// - The session is cached under `HOME` (see `default_token_cache_path`).
///   Without `HOME` it is not cached.
///
/// Returns:
///     The config.
///
/// Raises:
///     AsvoError: `MWA_ASVO_API_TIMEOUT` is set but is not a whole number of
///         seconds of at least 1 (kind `InvalidEnvironment`).
///     AsvoApiError: `MWA_ASVO_API_KEY` is not set (kind `MissingAuthKey`).
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn client_config_from_env(py: Python<'_>) -> PyResult<PyAsvoClientConfig> {
    crate::client_config_from_env()
        .map(PyAsvoClientConfig::from)
        .map_err(|e| asvo_error(py, e))
}

/// The settings of a download that the environment can give: how much data to
/// hold in memory before it is written, and how long to retry a failing
/// download.
///
/// Args:
///     buffer_size: How many bytes to hold in memory before they are
///         written. At least `MIN_DOWNLOAD_BUFFER_SIZE`. `None` is
///         `DEFAULT_DOWNLOAD_BUFFER_SIZE`.
///     retry_duration: How long to retry a failing download, in seconds. 0
///         disables retries. `None` is `DEFAULT_DOWNLOAD_RETRY_DURATION`.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(name = "DownloadSettings", module = "mwa_giant_squid")]
pub struct PyDownloadSettings {
    /// How many bytes to hold in memory before they are written.
    #[pyo3(get, set)]
    pub buffer_size: usize,
    /// How long to retry a failing download, in seconds.
    #[pyo3(get, set)]
    pub retry_duration: f64,
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyDownloadSettings {
    #[new]
    #[pyo3(signature = (buffer_size=None, retry_duration=None))]
    fn new(buffer_size: Option<usize>, retry_duration: Option<f64>) -> Self {
        Self {
            buffer_size: buffer_size.unwrap_or(DEFAULT_DOWNLOAD_BUFFER_SIZE),
            retry_duration: retry_duration
                .unwrap_or_else(|| DEFAULT_DOWNLOAD_RETRY_DURATION.as_secs_f64()),
        }
    }

    /// Read the settings from the environment, as the `giant-squid` command
    /// does.
    ///
    /// - `GIANT_SQUID_BUF_SIZE` is a whole number of MiB. The default is
    ///   `DEFAULT_DOWNLOAD_BUFFER_SIZE`.
    /// - `GIANT_SQUID_DOWNLOAD_RETRY_SECS` is a whole number of seconds. The
    ///   default is `DEFAULT_DOWNLOAD_RETRY_DURATION`.
    ///
    /// Returns:
    ///     The settings.
    ///
    /// Raises:
    ///     AsvoError: A variable is set but is not a whole number, or is too
    ///         large (kind `InvalidEnvironment`).
    #[staticmethod]
    fn from_env(py: Python<'_>) -> PyResult<Self> {
        DownloadSettings::from_env()
            .map(|settings| Self {
                buffer_size: settings.buffer_size,
                retry_duration: settings.retry_duration.as_secs_f64(),
            })
            .map_err(|e| asvo_error(py, e))
    }

    fn __repr__(&self) -> String {
        format!(
            "DownloadSettings(buffer_size={}, retry_duration={})",
            self.buffer_size, self.retry_duration
        )
    }
}
