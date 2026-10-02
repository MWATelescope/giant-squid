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

use std::sync::{Once, OnceLock};

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

/// The constants that the module shares with the programs on top of it (the
/// `giant-squid` command in Python is one). Each is `(name, value)`. They are
/// the library's own constants, so a program does not repeat them.
macro_rules! string_constants {
    ($($name:ident),+ $(,)?) => {
        /// The string constants of the module, by name.
        const STRING_CONSTANTS: &[(&str, &str)] = &[$((stringify!($name), crate::$name)),+];

        $(
            #[cfg(feature = "python-stubgen")]
            pyo3_stub_gen::module_variable!("mwa_giant_squid", stringify!($name), String);
        )+
    };
}

string_constants!(
    ENV_GIANT_SQUID_DELIVERY,
    ENV_GIANT_SQUID_DELIVERY_FORMAT,
    ENDPOINT_JOBS,
    ENDPOINT_CONVERSION_JOB,
    ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_VOLTAGE_JOB,
    ENDPOINT_BEAMFORMER_JOB,
    ENDPOINT_IMAGING_JOB,
    ENDPOINT_IMAGE_FROM_JOB,
);

#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "WAIT_POLL_INTERVAL_SECS", f64);
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "WAIT_INITIAL_DELAY_SECS", f64);
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "DEFAULT_CONCURRENT_DOWNLOADS", usize);

/// The handle that clears `pyo3-log`'s cache of Python loggers and levels.
/// Set once, when [`connect_python_logging`] first runs.
static LOG_RESET_HANDLE: OnceLock<pyo3_log::ResetHandle> = OnceLock::new();

/// Make the Rust log records go to Python's `logging`.
///
/// Only one Rust logger can be installed per process, and the `giant-squid`
/// command (`_run_cli`) needs to install its own, with progress bars. So the
/// module does not install `pyo3-log` when it is imported. It installs it
/// here, the first time that something that can log is used: an `AsvoClient`
/// is made, or the settings are read from the environment. Calls after the
/// first do nothing.
///
/// Another extension module in this process may have installed a Rust logger
/// already (or the `giant-squid` command, in this process). Then the records
/// go to that logger, and there is nothing to reset.
pub(crate) fn connect_python_logging() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| match pyo3_log::try_init() {
        Ok(handle) => {
            let _ = LOG_RESET_HANDLE.set(handle);
        }
        Err(e) => log::debug!("A Rust logger is already installed; not installing pyo3-log: {e}"),
    });
}

/// Python bindings for giant-squid, a client for the MWA ASVO.
#[pymodule(name = "mwa_giant_squid")]
mod module {
    use pyo3::prelude::*;

    use super::LOG_RESET_HANDLE;

    #[pymodule_export]
    use super::client::PyAsvoClient;
    #[pymodule_export]
    use super::error::{AsvoApiError, AsvoError};
    #[pymodule_export]
    use super::functions::{
        beamformer_job_params, conversion_job_params, download_meta_job_params,
        download_vis_job_params, image_from_job_params, imaging_job_params, parse_job_ids_only,
        parse_many_job_ids_or_obs_ids, parse_obs_ids_only, parse_utc_time, voltage_job_params,
    };
    #[pymodule_export]
    use super::types::{
        PyAsvoFilesArray, PyAsvoJob, PyAsvoJobProduct, PyAsvoJobState, PyAsvoJobType, PyAsvoJobVec,
        PyCentre, PyDelivery, PyDeliveryFormat, PyDownloadProgress, PyDownloadSettings,
        PyJobSubmittedResponse, PyOutput, PyOutputMode, PyPolarization, PyWeighting,
    };

    /// Make Python see changes to its logging configuration.
    ///
    /// For speed, the module caches each Python logger and its level the
    /// first time a Rust log record uses it. Call this after you change
    /// the logging configuration (for example, after `logging.basicConfig`
    /// or `setLevel`), if the module has already logged. The module connects
    /// to Python's `logging` when you make the first `AsvoClient` or read the
    /// settings from the environment, so until then there is nothing to reset.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    fn reset_logging() {
        if let Some(handle) = LOG_RESET_HANDLE.get() {
            handle.reset();
        }
    }

    /// Run the `giant-squid` command line program, which is written in Rust,
    /// and return its exit code.
    ///
    /// This is what the Python `giant-squid` program calls, so that it is
    /// the same program as the Rust one. It is not part of the library API
    /// and may change.
    ///
    /// Args:
    ///     args: The arguments, the first of which is the name of the program.
    ///
    /// Returns:
    ///     The exit code: 0 for success (also for `--help` and `--version`),
    ///     2 for a bad argument, 1 for any other error. The output and the
    ///     errors are written to the real standard output and standard error
    ///     of the process, not to `sys.stdout` and `sys.stderr`.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    #[pyo3(name = "_run_cli")]
    fn run_cli(py: Python<'_>, args: Vec<String>) -> i32 {
        // The command does not use Python, so let other threads run.
        py.detach(|| crate::cli::run::run_cli(args))
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        for (name, value) in super::STRING_CONSTANTS {
            m.add(*name, *value)?;
        }
        m.add(
            "WAIT_POLL_INTERVAL_SECS",
            crate::WAIT_POLL_INTERVAL.as_secs_f64(),
        )?;
        m.add(
            "WAIT_INITIAL_DELAY_SECS",
            crate::WAIT_INITIAL_DELAY.as_secs_f64(),
        )?;
        m.add(
            "DEFAULT_CONCURRENT_DOWNLOADS",
            crate::DEFAULT_CONCURRENT_DOWNLOADS,
        )?;
        Ok(())
    }
}
