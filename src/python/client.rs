// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `AsvoClient`.
//!
//! Every network call releases the GIL (`Python::detach`), so other Python
//! threads run while it waits. The library client is `Send + Sync`, so one
//! Python `AsvoClient` can be used from several threads.

use std::path::PathBuf;
use std::time::Duration;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::error::api_error;
use super::types::PyAsvoJobVec;
use crate::asvo::{AsvoClient, AsvoClientConfig};

/// A client for the MWA ASVO. It logs in when it is created.
///
/// Args:
///     host: The MWA ASVO base URL, for example
///         "https://asvo.mwatelescope.org:443". An "http://" host is
///         permitted (a local test server); every other host must use TLS.
///     api_key: Your MWA ASVO API key.
///     api_timeout: The timeout for one API request, in seconds. `None`
///         uses the library default (60 s).
///     token_cache_path: Where to cache the session between runs. `None`
///         keeps the session in memory only, so every new client logs in.
///         Give a path for a script that runs often, because the server
///         permits only a few logins a minute.
///
/// Raises:
///     AsvoApiError: The API key is empty, or the login failed.
///     ValueError: `api_timeout` is negative or not finite.
#[pyclass(frozen, name = "AsvoClient", module = "mwa_giant_squid")]
pub struct PyAsvoClient {
    inner: AsvoClient,
    host: String,
}

#[pymethods]
impl PyAsvoClient {
    #[new]
    #[pyo3(signature = (host, api_key, *, api_timeout=None, token_cache_path=None))]
    fn new(
        py: Python<'_>,
        host: String,
        api_key: String,
        api_timeout: Option<f64>,
        token_cache_path: Option<PathBuf>,
    ) -> PyResult<Self> {
        let mut config = AsvoClientConfig::new(host.clone(), api_key);
        if let Some(seconds) = api_timeout {
            config.api_timeout = Duration::try_from_secs_f64(seconds).map_err(|e| {
                PyValueError::new_err(format!("api_timeout={seconds} is not valid: {e}"))
            })?;
        }
        config.token_cache_path = token_cache_path;

        let inner = py
            .detach(|| AsvoClient::new(config))
            .map_err(|e| api_error(py, e))?;
        Ok(Self { inner, host })
    }

    /// Get your jobs.
    ///
    /// Args:
    ///     days: Only the jobs from the past `days` days. `None` gets your
    ///         full job history.
    ///
    /// Raises:
    ///     AsvoApiError: The request failed.
    #[pyo3(signature = (days=None))]
    fn get_jobs(&self, py: Python<'_>, days: Option<i64>) -> PyResult<PyAsvoJobVec> {
        py.detach(|| self.inner.get_jobs(days))
            .map(PyAsvoJobVec::from)
            .map_err(|e| api_error(py, e))
    }

    fn __repr__(&self) -> String {
        format!("AsvoClient(host={:?})", self.host)
    }
}
