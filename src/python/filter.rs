// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Python class `JobsFilter`: the library's [`JobsFilter`].
//!
//! The class holds a library `JobsFilter`, so each value is checked when it
//! is set, as its Rust type is: an Obs ID is an `ObsId`, a Job ID is not 0,
//! and `days` is not 0 or outside the API's limits (the library's
//! [`validate::days`]).

use jiff::Timestamp;
use pyo3::prelude::*;

use super::error::api_error;
use super::obs_id::{ObsIdArg, PyObsId};
use super::typed::JobId;
use super::types::{PyJobState, PyJobType, PySortBy};
use crate::mwa_asvo::api::validate;
use crate::mwa_asvo::JobsFilter;

/// Which of your jobs to get: the filter of `AsvoClient.get_jobs`.
///
/// A filter that is `None` or empty does not filter, except `days` and
/// `sort_by`: `days=None` is the default window of the MWA ASVO API (30
/// days), not your full history. The server filters by `days`, the dates
/// and the sort order, and the lists (Job IDs, Obs IDs, types and states) are
/// applied to the result.
///
/// Args:
///     days: Only the jobs from the past `days` days, from 1 to 30. Cannot
///         be given with `date_from` or `date_to`: the jobs are selected by
///         `days` or by the dates. With a date and no `days`, `days` is sent
///         as `null`.
///     date_from: Only the jobs created at or after this time. It must have
///         a time zone.
///     date_to: Only the jobs created at or before this time. It must have a
///         time zone.
///     sort_by: The column to sort the jobs by (a `SortBy`). `None` uses the
///         server's default order.
///     job_ids: Only these jobs. Cannot be combined with `obs_ids`.
///     obs_ids: Only the jobs for these Obs IDs.
///     job_types: Only the jobs of these types.
///     job_states: Only the jobs in these states.
///
/// Raises:
///     ObsIdError: An Obs ID is not valid.
///     ValueError: A Job ID is 0, or `days` is outside the limits of the API.
///     TypeError: `date_from` or `date_to` has no time zone.
#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(skip_from_py_object, name = "JobsFilter", module = "mwa_giant_squid")]
#[derive(Clone, Default)]
pub struct PyJobsFilter(pub JobsFilter);

impl PyJobsFilter {
    /// Set `days`, checked by the library's [`validate::days`].
    fn check_days(&mut self, py: Python<'_>, days: Option<i64>) -> PyResult<()> {
        self.0.days = days
            .map(validate::days)
            .transpose()
            .map_err(|e| api_error(py, e))?;
        Ok(())
    }
}

#[cfg_attr(feature = "python-stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PyJobsFilter {
    #[new]
    #[pyo3(signature = (
        *,
        days=None,
        date_from=None,
        date_to=None,
        sort_by=None,
        job_ids=None,
        obs_ids=None,
        job_types=None,
        job_states=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        days: Option<i64>,
        date_from: Option<Timestamp>,
        date_to: Option<Timestamp>,
        sort_by: Option<PySortBy>,
        job_ids: Option<Vec<JobId>>,
        obs_ids: Option<Vec<ObsIdArg>>,
        job_types: Option<Vec<PyJobType>>,
        job_states: Option<Vec<PyJobState>>,
    ) -> PyResult<Self> {
        let mut filter = Self::default();
        filter.check_days(py, days)?;
        filter.0.date_from = date_from;
        filter.0.date_to = date_to;
        filter.0.sort_by = sort_by.map(Into::into);
        filter.set_job_ids(job_ids.unwrap_or_default());
        filter.set_obs_ids(obs_ids.unwrap_or_default());
        filter.set_job_types(job_types.unwrap_or_default());
        filter.set_job_states(job_states.unwrap_or_default());
        Ok(filter)
    }

    /// Only the jobs from the past `days` days, from 1 to 30, or `None`.
    #[getter]
    fn days(&self) -> Option<u64> {
        self.0.days.map(std::num::NonZeroU64::get)
    }

    #[setter]
    fn set_days(&mut self, days: Option<i64>) -> PyResult<()> {
        Python::attach(|py| self.check_days(py, days))
    }

    /// Only the jobs created at or after this time, or `None`.
    #[getter]
    fn date_from(&self) -> Option<Timestamp> {
        self.0.date_from
    }

    #[setter]
    fn set_date_from(&mut self, date_from: Option<Timestamp>) {
        self.0.date_from = date_from;
    }

    /// Only the jobs created at or before this time, or `None`.
    #[getter]
    fn date_to(&self) -> Option<Timestamp> {
        self.0.date_to
    }

    #[setter]
    fn set_date_to(&mut self, date_to: Option<Timestamp>) {
        self.0.date_to = date_to;
    }

    /// The column to sort the jobs by, or `None`.
    #[getter]
    fn sort_by(&self) -> Option<PySortBy> {
        self.0.sort_by.map(PySortBy::from)
    }

    #[setter]
    fn set_sort_by(&mut self, sort_by: Option<PySortBy>) {
        self.0.sort_by = sort_by.map(Into::into);
    }

    /// Only these jobs. Cannot be combined with `obs_ids`.
    #[getter]
    fn job_ids(&self) -> Vec<JobId> {
        self.0.job_ids.iter().copied().map(JobId).collect()
    }

    #[setter]
    fn set_job_ids(&mut self, job_ids: Vec<JobId>) {
        self.0.job_ids = job_ids.into_iter().map(|id| id.0).collect();
    }

    /// Only the jobs for these Obs IDs. Cannot be combined with `job_ids`.
    #[getter]
    fn obs_ids(&self) -> Vec<PyObsId> {
        self.0.obs_ids.iter().copied().map(PyObsId).collect()
    }

    #[setter]
    fn set_obs_ids(&mut self, obs_ids: Vec<ObsIdArg>) {
        self.0.obs_ids = obs_ids.into_iter().map(|o| o.0).collect();
    }

    /// Only the jobs of these types.
    #[getter]
    fn job_types(&self) -> Vec<PyJobType> {
        self.0
            .job_types
            .iter()
            .copied()
            .map(PyJobType::from)
            .collect()
    }

    #[setter]
    fn set_job_types(&mut self, job_types: Vec<PyJobType>) {
        self.0.job_types = job_types.into_iter().map(Into::into).collect();
    }

    /// Only the jobs in these states.
    #[getter]
    fn job_states(&self) -> Vec<PyJobState> {
        self.0
            .job_states
            .iter()
            .copied()
            .map(PyJobState::from)
            .collect()
    }

    #[setter]
    fn set_job_states(&mut self, job_states: Vec<PyJobState>) {
        self.0.job_states = job_states.into_iter().map(Into::into).collect();
    }

    /// Check the filter before any request. `AsvoClient.get_jobs` does this.
    ///
    /// Raises:
    ///     ValueError: Both `job_ids` and `obs_ids` are given, `days` is not
    ///         from 1 to 30, or `days` is given with `date_from` or
    ///         `date_to`.
    fn validate(&self, py: Python<'_>) -> PyResult<()> {
        self.0.validate().map_err(|e| api_error(py, e))
    }

    fn __repr__(&self) -> String {
        let f = &self.0;
        // Only the values that are set, as the constructor takes them.
        let mut parts: Vec<String> = Vec::new();
        if let Some(days) = f.days {
            parts.push(format!("days={days}"));
        }
        if let Some(t) = f.date_from {
            parts.push(format!("date_from={t}"));
        }
        if let Some(t) = f.date_to {
            parts.push(format!("date_to={t}"));
        }
        if let Some(s) = f.sort_by {
            parts.push(format!("sort_by={s}"));
        }
        let list = |items: Vec<String>| format!("[{}]", items.join(", "));
        if !f.job_ids.is_empty() {
            parts.push(format!(
                "job_ids={}",
                list(f.job_ids.iter().map(ToString::to_string).collect())
            ));
        }
        if !f.obs_ids.is_empty() {
            parts.push(format!(
                "obs_ids={}",
                list(f.obs_ids.iter().map(ToString::to_string).collect())
            ));
        }
        if !f.job_types.is_empty() {
            parts.push(format!(
                "job_types={}",
                list(f.job_types.iter().map(ToString::to_string).collect())
            ));
        }
        if !f.job_states.is_empty() {
            parts.push(format!(
                "job_states={}",
                list(f.job_states.iter().map(ToString::to_string).collect())
            ));
        }
        format!("JobsFilter({})", parts.join(", "))
    }
}
