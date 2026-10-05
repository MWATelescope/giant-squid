// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Downloads from Python: the progress callback, and Ctrl-C.
//!
//! A download runs with the GIL released (`Python::detach`). The library
//! calls two hooks from that thread: `progress` with each event, and
//! `should_stop` before each chunk and while it waits to retry. Each hook
//! takes the GIL again (`Python::attach`) only when it has to:
//!
//! - `should_stop` runs `Python::check_signals` at most once per
//!   [`CALLBACK_INTERVAL`]. So Ctrl-C (`KeyboardInterrupt`) stops the
//!   download at the next chunk, when the download runs on the main thread,
//!   which is where Python runs signal handlers.
//! - `progress` sends `Started` and `Finished` at once. It adds the bytes of
//!   `Advanced` events together and sends them at most once per
//!   [`CALLBACK_INTERVAL`], because the library reports every chunk (many
//!   thousand a second on a fast link), and one Python call per chunk would
//!   slow the download. The sum of the `bytes` is unchanged.
//!
//! If the callback raises, or a signal handler raises, the exception is
//! kept, the download stops at the next check, and the exception is raised
//! to the caller of the download. It takes priority over the error that
//! the stopped download returns.
//!
//! A signal handler does not only run inside `check_signals`. Python runs a
//! pending one at the next bytecode of the main thread, and the download makes
//! Python calls of its own: every log record goes through Python's `logging`
//! (pyo3-log). When Ctrl-C lands in one of those calls, the `KeyboardInterrupt`
//! leaves the log call, and pyo3-log, which cannot return an error from a log
//! call, leaves it as the current exception of the thread. The signal is spent
//! by then, so `check_signals` finds nothing, and without [`take_stray_error`]
//! the download would carry on, and even finish, as if Ctrl-C had not been
//! pressed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use super::error::asvo_error;
use super::types::PyDownloadProgress;
use crate::asvo::{
    AsvoError, DownloadOptions, DownloadProgress, DEFAULT_DOWNLOAD_BUFFER_SIZE,
    DEFAULT_DOWNLOAD_RETRY_DURATION,
};

/// The shortest time between two calls into Python for `Advanced` events,
/// and between two checks for signals.
const CALLBACK_INTERVAL: Duration = Duration::from_millis(100);

/// The download options that a Python caller gives, before they become a
/// library [`DownloadOptions`].
pub(super) struct PyDownloadArgs {
    pub download_dir: PathBuf,
    pub keep_tar: bool,
    pub no_resume: bool,
    pub hash: bool,
    pub progress: Option<Py<PyAny>>,
    pub buffer_size: Option<usize>,
    pub retry_duration: Option<f64>,
    pub download_number: usize,
    pub download_count: usize,
}

/// The state that the two hooks share.
struct Hooks {
    /// The Python progress callback, if any.
    callback: Option<Py<PyAny>>,
    /// Set when `error` holds an exception, so the fast path of
    /// `should_stop` needs no lock.
    failed: AtomicBool,
    /// The first exception from the callback or from a signal handler.
    error: Mutex<Option<PyErr>>,
    /// The `Advanced` bytes not sent to the callback yet, and when the
    /// callback was last called with an `Advanced` event.
    pending: Mutex<(u64, Instant)>,
    /// When signals were last checked. `None` before the first check.
    last_signal_check: Mutex<Option<Instant>>,
}

impl Hooks {
    fn new(callback: Option<Py<PyAny>>) -> Self {
        Self {
            callback,
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
            pending: Mutex::new((0, Instant::now())),
            last_signal_check: Mutex::new(None),
        }
    }

    /// Keep `err` if it is the first, and make the download stop.
    fn fail(&self, err: PyErr) {
        let mut error = self.error.lock().unwrap_or_else(|p| p.into_inner());
        if error.is_none() {
            *error = Some(err);
        }
        self.failed.store(true, Ordering::SeqCst);
    }

    /// Call the Python callback with `event`, and keep any exception.
    fn call(&self, event: PyDownloadProgress) {
        let Some(callback) = &self.callback else {
            return;
        };
        if self.failed.load(Ordering::SeqCst) {
            return;
        }
        Python::attach(|py| {
            if let Err(err) = callback.call1(py, (event,)) {
                self.fail(err);
            }
        });
    }

    /// Send the `Advanced` bytes that are waiting, if there are any.
    fn flush(&self) {
        let bytes = {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            let bytes = pending.0;
            *pending = (0, Instant::now());
            bytes
        };
        if bytes > 0 {
            self.call(PyDownloadProgress::Advanced { bytes });
        }
    }

    /// The library's `progress` hook.
    fn progress(&self, event: DownloadProgress) {
        if self.callback.is_none() {
            return;
        }
        match event {
            DownloadProgress::Advanced { bytes } => {
                let due = {
                    let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
                    pending.0 += bytes;
                    pending.1.elapsed() >= CALLBACK_INTERVAL
                };
                if due {
                    self.flush();
                }
            }
            other => {
                // Keep the order of events: bytes of the previous file come
                // before this file's Started, and before this Finished.
                self.flush();
                self.call(other.into());
            }
        }
    }

    /// The library's `should_stop` hook.
    fn should_stop(&self) -> bool {
        if self.failed.load(Ordering::SeqCst) {
            return true;
        }
        let due = {
            let mut last = self
                .last_signal_check
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let due = last.is_none_or(|t| t.elapsed() >= CALLBACK_INTERVAL);
            if due {
                *last = Some(Instant::now());
            }
            due
        };
        if due {
            Python::attach(|py| {
                // First, an exception that a log call left behind (see the
                // module documentation): its signal has been handled.
                if let Some(err) = take_stray_error(py) {
                    self.fail(err);
                } else if let Err(err) = py.check_signals() {
                    self.fail(err);
                }
            });
        }
        self.failed.load(Ordering::SeqCst)
    }

    /// The exception to raise, if the callback or a signal handler raised.
    fn take_error(&self) -> Option<PyErr> {
        self.error.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
}

/// The exception that a Python call made by the library left as the current
/// exception of the thread, if it is one that should stop the download.
///
/// pyo3-log does this when a log call raises (see the module documentation).
/// An exception that is not an `Exception` (`KeyboardInterrupt`, `SystemExit`:
/// what a signal handler raises to end the program) is returned, so that the
/// download stops and the caller gets it. An ordinary `Exception` is a fault
/// of the logging set-up, and logging must not break a download: it is cleared
/// and dropped, as `logging` itself would handle it.
fn take_stray_error(py: Python<'_>) -> Option<PyErr> {
    let err = PyErr::take(py)?;
    if err.is_instance_of::<PyException>(py) {
        None
    } else {
        Some(err)
    }
}

/// Run `download` with the options in `args`, with the GIL released.
///
/// `download` is the library call (`download_job` or `download_obs`).
/// An exception from the progress callback or from Ctrl-C is raised in
/// place of any error of the download.
///
/// # Errors
///
/// `ValueError` for a `download_dir` that is not UTF-8 or a bad
/// `retry_duration`; `AsvoError` or `AsvoApiError` from the download; or
/// the exception from the callback or a signal handler.
pub(super) fn run_download<F, T>(py: Python<'_>, args: PyDownloadArgs, download: F) -> PyResult<T>
where
    F: FnOnce(&DownloadOptions) -> Result<T, AsvoError> + Send,
    T: Send,
{
    let download_dir = args
        .download_dir
        .to_str()
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "download_dir={:?} is not valid UTF-8",
                args.download_dir
            ))
        })?
        .to_string();
    let retry_duration = match args.retry_duration {
        None => DEFAULT_DOWNLOAD_RETRY_DURATION,
        Some(seconds) => Duration::try_from_secs_f64(seconds).map_err(|e| {
            PyValueError::new_err(format!("retry_duration={seconds} is not valid: {e}"))
        })?,
    };
    let hooks = Hooks::new(args.progress);

    let result = py.detach(|| {
        let progress = |event: DownloadProgress| hooks.progress(event);
        let should_stop = || hooks.should_stop();
        let opts = DownloadOptions {
            keep_tar: args.keep_tar,
            no_resume: args.no_resume,
            hash: args.hash,
            download_dir: &download_dir,
            progress: Some(&progress),
            download_number: args.download_number,
            download_count: args.download_count,
            buffer_size: args.buffer_size.unwrap_or(DEFAULT_DOWNLOAD_BUFFER_SIZE),
            retry_duration,
            should_stop: Some(&should_stop),
        };
        let result = download(&opts);
        // Bytes still waiting are sent before the caller sees the result.
        hooks.flush();
        result
    });

    if let Some(err) = hooks.take_error() {
        return Err(err);
    }
    // A log call after the last check may have left one too. Without this it
    // would stay set, and Python would see it on an unrelated call.
    if let Some(err) = take_stray_error(py) {
        return Err(err);
    }
    result.map_err(|e| asvo_error(py, e))
}
