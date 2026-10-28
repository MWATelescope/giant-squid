// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The functions of the library that read Job IDs, Obs IDs and times.
//!
//! Each calls the library function of the same name. An error is a
//! `ParseError` (see [`super::error::parse_error`]).

use std::path::PathBuf;

use pyo3::prelude::*;

use super::error::parse_error;
use super::obs_id::PyObsId;
use super::typed::JobId;

/// Read Obs IDs and files of Obs IDs, for a command that takes Obs IDs only.
///
/// Each string is an Obs ID, or the name of a file of IDs that spaces or
/// lines separate. Read a number that has 10 digits as an Obs ID, and any
/// other number as a Job ID.
///
/// Args:
///     strings: Obs IDs, and files of them.
///
/// Returns:
///     The Obs IDs, in order.
///
/// Raises:
///     ParseError: A Job ID is given anywhere, even with Obs IDs (kind
///         `JobIdsGiven`), no Obs ID is given (kind `NoObsIds`), a word is
///         not a number (kind `InsideFile`), or an ID is 0 (kind
///         `ZeroJobId`).
///     OSError: A file cannot be read.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn parse_obs_ids_only(py: Python<'_>, strings: Vec<String>) -> PyResult<Vec<PyObsId>> {
    py.detach(|| crate::parse_obs_ids_only(&strings))
        .map(|ids| ids.into_iter().map(PyObsId).collect())
        .map_err(|e| parse_error(py, e))
}

/// Read Job IDs and files of Job IDs, for a command that takes Job IDs only.
///
/// Each string is a Job ID, or the name of a file of IDs that spaces or
/// lines separate.
///
/// Args:
///     strings: Job IDs, and files of them.
///
/// Returns:
///     The Job IDs, in order.
///
/// Raises:
///     ParseError: An Obs ID is given anywhere, even with Job IDs (kind
///         `ObsIdsGiven`), no Job ID is given (kind `NoJobIds`), a word is
///         not a number (kind `InsideFile`), or an ID is 0 (kind
///         `ZeroJobId`).
///     OSError: A file cannot be read.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn parse_job_ids_only(py: Python<'_>, strings: Vec<String>) -> PyResult<Vec<JobId>> {
    py.detach(|| crate::parse_job_ids_only(&strings))
        .map(|ids| ids.into_iter().map(JobId).collect())
        .map_err(|e| parse_error(py, e))
}

/// Read Job IDs, Obs IDs, and files of them.
///
/// A number that has 10 digits is an Obs ID, and any other number is a Job
/// ID. A string that is not a number is the name of a file of IDs that
/// spaces or lines separate.
///
/// Args:
///     strings: Job IDs, Obs IDs, and files of them.
///
/// Returns:
///     The Job IDs and the Obs IDs, each in order.
///
/// Raises:
///     ParseError: A word in a file is not a number (kind `InsideFile`),
///         or an ID is 0 (kind `ZeroJobId`).
///     OSError: A file cannot be read.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn parse_many_job_ids_or_obs_ids(
    py: Python<'_>,
    strings: Vec<String>,
) -> PyResult<(Vec<JobId>, Vec<PyObsId>)> {
    py.detach(|| crate::parse_many_job_ids_or_obs_ids(&strings))
        .map(|(jobs, obs)| {
            (
                jobs.into_iter().map(JobId).collect(),
                obs.into_iter().map(PyObsId).collect(),
            )
        })
        .map_err(|e| parse_error(py, e))
}

/// Read the Job IDs and the Obs IDs of a file.
///
/// The IDs are separated by spaces or lines. A number that has 10 digits is
/// an Obs ID, and any other number is a Job ID.
///
/// Args:
///     f: The file.
///
/// Returns:
///     The Job IDs and the Obs IDs, each in order.
///
/// Raises:
///     ParseError: A word in the file is not a number (kind `InsideFile`),
///         or an ID is 0 (kind `ZeroJobId`).
///     OSError: The file cannot be read.
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn parse_job_ids_and_obs_ids_from_file(
    py: Python<'_>,
    f: PathBuf,
) -> PyResult<(Vec<JobId>, Vec<PyObsId>)> {
    py.detach(|| crate::parse_job_ids_and_obs_ids_from_file(&f))
        .map(|(jobs, obs)| {
            (
                jobs.into_iter().map(JobId).collect(),
                obs.into_iter().map(PyObsId).collect(),
            )
        })
        .map_err(|e| parse_error(py, e))
}

/// Read a time for the `date_from` and `date_to` of a job list.
///
/// The text is RFC 3339 (for example `2026-09-01T00:00:00Z`), or a date
/// alone (`2026-09-01`), which is midnight UTC. A date and time with no
/// offset (`2026-09-01T12:00:00`) is refused and not guessed.
///
/// Args:
///     text: The time.
///
/// Returns:
///     The time, as a `datetime.datetime` that has the UTC time zone.
///
/// Raises:
///     ParseError: The text is not a time (kind `InvalidTime`).
#[cfg_attr(
    feature = "python-stubgen",
    pyo3_stub_gen::derive::gen_stub_pyfunction(module = "mwa_giant_squid")
)]
#[pyfunction]
pub(super) fn parse_utc_time(py: Python<'_>, text: &str) -> PyResult<jiff::Timestamp> {
    crate::parse_utc_time(text).map_err(|e| parse_error(py, e))
}
