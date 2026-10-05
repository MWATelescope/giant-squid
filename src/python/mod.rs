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
// `__version__` and `_PROGRAM_NAME` are added in `init`, not with a
// `#[pymodule_export]`, so they are declared here for the stubs.
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "__version__", String);
#[cfg(feature = "python-stubgen")]
pyo3_stub_gen::module_variable!("mwa_giant_squid", "_PROGRAM_NAME", String);

/// The handle that clears `pyo3-log`'s cache of Python loggers and levels.
/// Set once, when [`connect_python_logging`] first runs.
static LOG_RESET_HANDLE: OnceLock<pyo3_log::ResetHandle> = OnceLock::new();

/// Make the Rust log records go to Python's `logging`.
///
/// Only one Rust logger can be installed per process, and the `giant-squid`
/// command (`_run_cli`) needs to install its own, with progress bars. So the
/// module does not install `pyo3-log` when it is imported. It installs it
/// here, the first time that something that can log is used: an `AsvoClient`
/// is made. Calls after the first do nothing.
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
    use super::types::{
        PyAsvoJob, PyAsvoJobVec, PyCentre, PyDelivery, PyDeliveryFormat, PyDownloadProgress,
        PyJobFile, PyJobProduct, PyJobState, PyJobSubmittedResponse, PyJobType, PyOutput,
        PyOutputMode, PyPolarization, PyType, PyWeighting,
    };

    /// Make Python see changes to its logging configuration.
    ///
    /// For speed, the module caches each Python logger and its level the
    /// first time a Rust log record uses it. Call this after you change
    /// the logging configuration (for example, after `logging.basicConfig`
    /// or `setLevel`), if the module has already logged. The module connects
    /// to Python's `logging` when you make the first `AsvoClient`, so until
    /// then there is nothing to reset.
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
        // The name of the `giant-squid` program, which the Python launcher
        // gives to `_run_cli` as the first argument. Not part of the API.
        m.add("_PROGRAM_NAME", crate::cli::PROGRAM_NAME)?;
        Ok(())
    }
}
