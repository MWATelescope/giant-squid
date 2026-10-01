// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python module `mwa_giant_squid`, built with PyO3.
//!
//! Build it with maturin, which reads its settings from `pyproject.toml`:
//!
//! ```text
//! maturin develop        # build and install into the active virtualenv
//! maturin build          # build a wheel
//! ```
//!
//! All binding code is in this directory, behind the `python` feature. The
//! Rust library does not depend on it.
//!
//! Rust `log` records go to Python `logging` through `pyo3-log`. The logger
//! names are the Rust module paths with `::` changed to `.`, so the
//! `mwa_giant_squid` logger is the parent of all of them.

// The pyo3-stub-gen macros put `TypeId::of::<T>` in statics as a function
// pointer (it is not called there). Clippy reports this as a use of a
// `const` feature from Rust 1.91, above the crate's MSRV; a pointer to a
// non-const function is permitted in a static on older Rust too. Only the
// maintainer-only "python-stubgen" builds have these statics.
#![cfg_attr(feature = "python-stubgen", allow(clippy::incompatible_msrv))]

mod client;
mod download;
mod error;
mod functions;
mod params;
mod typed;
mod types;

use std::sync::OnceLock;

use pyo3::prelude::*;

// The function that the `stub_gen` binary calls. It reads the module name
// from `pyproject.toml`.
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::define_stub_info_gatherer!(stub_info);
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_doc!(
    "mwa_giant_squid",
    "Python bindings for giant-squid, a client for the MWA ASVO."
);
// `__version__` is added in `init`, not with a `#[pymodule_export]`, so it
// is declared here for the stubs.
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "__version__", String);

/// The handle that clears `pyo3-log`'s cache of Python loggers and levels.
/// Set once, when the module is first imported.
static LOG_RESET_HANDLE: OnceLock<pyo3_log::ResetHandle> = OnceLock::new();

/// Python bindings for giant-squid, a client for the MWA ASVO.
#[pymodule(name = "mwa_giant_squid")]
mod module {
    use log::debug;
    use pyo3::prelude::*;

    use super::LOG_RESET_HANDLE;

    #[pymodule_export]
    use super::client::PyAsvoClient;
    #[pymodule_export]
    use super::error::{AsvoApiError, AsvoError};
    #[pymodule_export]
    use super::functions::{
        beamformer_job_params, conversion_job_params, download_meta_job_params,
        download_vis_job_params, image_from_job_params, imaging_job_params,
        parse_many_job_ids_or_obs_ids, voltage_job_params,
    };
    #[pymodule_export]
    use super::types::{
        PyAsvoFilesArray, PyAsvoJob, PyAsvoJobProduct, PyAsvoJobState, PyAsvoJobType, PyAsvoJobVec,
        PyCentre, PyDelivery, PyDeliveryFormat, PyDownloadProgress, PyJobSubmittedResponse,
        PyOutput, PyOutputMode, PyPolarization, PyWeighting,
    };

    /// Make Python see changes to its logging configuration.
    ///
    /// For speed, the module caches each Python logger and its level the
    /// first time a Rust log record uses it. Call this after you change
    /// the logging configuration (for example, after `logging.basicConfig`
    /// or `setLevel`), if the module has already logged.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    fn reset_logging() {
        if let Some(handle) = LOG_RESET_HANDLE.get() {
            handle.reset();
        }
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        // Another extension module in this process may already have set the
        // Rust logger (there is only one per process). Then our records go
        // to that logger, and there is nothing to reset.
        match pyo3_log::try_init() {
            Ok(handle) => {
                let _ = LOG_RESET_HANDLE.set(handle);
            }
            Err(e) => debug!("A Rust logger is already installed; not installing pyo3-log: {e}"),
        }

        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        Ok(())
    }
}
