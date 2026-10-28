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
mod constants;
mod csv;
mod download;
mod error;
mod job_params;
mod obs_id;
mod params;
mod parse;
mod typed;
mod types;

use std::sync::atomic::AtomicBool;
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

/// Set when `_run_cli` first runs: it may run only once in a process.
static CLI_HAS_RUN: AtomicBool = AtomicBool::new(false);

/// The message of a second `_run_cli` in a process.
const RUN_CLI_TWICE: &str = "the giant-squid command can run only once in a process: \
     run each command in a process of its own";

/// The message of `_run_cli` after an `AsvoClient` was made.
const RUN_CLI_AFTER_CLIENT: &str = "the giant-squid command cannot run in a process that has \
     made an AsvoClient: run it in a process of its own";

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

    use super::error::asvo_error;
    use super::typed::JobId;
    use super::{CLI_HAS_RUN, LOG_RESET_HANDLE, RUN_CLI_AFTER_CLIENT, RUN_CLI_TWICE};
    use pyo3::exceptions::PyRuntimeError;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    #[pymodule_export]
    use super::client::PyAsvoClient;
    #[pymodule_export]
    use super::csv::{CsvSubmitError, PyCsvJobFailure};
    #[pymodule_export]
    use super::error::{
        AsvoApiError, AsvoError, ObsIdError, ObsIdErrorParse, ObsIdErrorWrongNumDigits, ParseError,
        ParseErrorInsideFile, ParseErrorInsideStdin, ParseErrorInvalidTime, ParseErrorJobIdsGiven,
        ParseErrorNoJobIds, ParseErrorNoObsIds, ParseErrorObsIdsGiven, ParseErrorStdinError,
        ParseErrorStdinJson, ParseErrorZeroJobId,
    };
    #[pymodule_export]
    use super::job_params::{
        PyConversionJobParams, PyDownloadJobParams, PyImagingJobFlow1Params,
        PyImagingJobFlow2Params,
    };
    #[pymodule_export]
    use super::obs_id::PyObsId;
    #[pymodule_export]
    use super::parse::{
        parse_job_ids_and_obs_ids_from_file, parse_job_ids_only, parse_many_job_ids_or_obs_ids,
        parse_obs_ids_only, parse_utc_time,
    };
    #[pymodule_export]
    use super::types::{
        PyAsvoJob, PyAsvoJobVec, PyDeliveryDestination, PyDeliveryFormat, PyDownloadProgress,
        PyImageOutputMode, PyImageWeighting, PyJobCancelledResponse, PyJobDetailResponse,
        PyJobFile, PyJobProduct, PyJobState, PyJobSubmittedResponse, PyJobType, PyOutputFormat,
        PyPhaseCentre, PyPolarization, PySortBy, PyStatus, PyType,
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
    ///
    /// Raises:
    ///     RuntimeError: The command already ran in this process, or an
    ///         `AsvoClient` was made first. A process has only one Rust
    ///         logger: the command installs its own the first time, and a
    ///         second run (or a run after the module's Python logging was
    ///         installed) would write its log lines to the wrong place. Run
    ///         each command in a process of its own, as the `giant-squid`
    ///         program does.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    #[pyo3(name = "_run_cli")]
    fn run_cli(py: Python<'_>, args: Vec<String>) -> PyResult<i32> {
        if LOG_RESET_HANDLE.get().is_some() {
            return Err(PyRuntimeError::new_err(RUN_CLI_AFTER_CLIENT));
        }
        if CLI_HAS_RUN.swap(true, Ordering::SeqCst) {
            return Err(PyRuntimeError::new_err(RUN_CLI_TWICE));
        }
        // The command does not use Python, so let other threads run.
        Ok(py.detach(|| crate::cli::run::run_cli(args)))
    }

    /// The path of the token cache that mwa-cli also uses:
    /// `<home>/.mwa-asvo/tokens.json`. Give it as the `token_cache_path` of an
    /// `AsvoClient` to share sessions with mwa-cli.
    ///
    /// Args:
    ///     home: The home directory.
    ///
    /// Returns:
    ///     The path of the token cache file.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    fn default_token_cache_path(home: PathBuf) -> PathBuf {
        crate::default_token_cache_path(&home)
    }

    /// Check the SHA-1 hash of a file against an expected hash.
    ///
    /// The comparison ignores the case of the hexadecimal digits.
    ///
    /// Args:
    ///     filename: The file to hash.
    ///     expected_hash: The hash that the file must have.
    ///     job_id: The Job ID that the file belongs to. It is for the error.
    ///
    /// Raises:
    ///     AsvoError: The hash does not match (kind `HashMismatch`).
    ///     ValueError: `job_id` is 0.
    ///     OSError: The file cannot be read.
    #[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
    #[pyfunction]
    fn check_file_sha1_hash(
        py: Python<'_>,
        filename: PathBuf,
        expected_hash: &str,
        job_id: JobId,
    ) -> PyResult<()> {
        py.detach(|| crate::check_file_sha1_hash(&filename, expected_hash, job_id.0))
            .map_err(|e| asvo_error(py, e))
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        super::constants::add(m)?;
        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        // The name of the `giant-squid` program, which the Python launcher
        // gives to `_run_cli` as the first argument. Not part of the API.
        m.add("_PROGRAM_NAME", crate::cli::PROGRAM_NAME)?;
        Ok(())
    }
}
