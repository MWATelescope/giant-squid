// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Obs ID argument of the submit methods.
//!
//! A submit method takes the Obs ID as a Python `int` or `ObsId` and checks
//! it here, as the library does. A number that is not a valid Obs ID raises
//! `ObsIdError`. Its
//! other arguments have the types and the defaults of the OpenAPI schema
//! (the defaults come from the library's `job_params`), and a value that the
//! schema does not allow raises `ValueError` (see `super::error::api_error`)
//! before anything is sent.

use pyo3::prelude::*;

use super::error::obs_id_error;
use super::obs_id::RawObsId;
use crate::obs_id::ObsId;

/// The Obs ID argument of a submit method, checked.
///
/// # Errors
///
/// `ObsIdError` if `obs_id` is not a valid Obs ID.
pub(super) fn job_obs_id(py: Python<'_>, obs_id: RawObsId) -> PyResult<ObsId> {
    ObsId::validate(obs_id.0).map_err(|e| obs_id_error(py, e))
}
