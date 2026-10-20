// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Obs ID argument of the submit methods.
//!
//! A submit method takes the Obs ID as a Python `int` and checks it here. Its
//! other arguments have the types and the defaults of the OpenAPI schema
//! (the defaults come from the library's `job_params`), and a value that the
//! schema does not allow raises `ValueError` (see `super::error::api_error`)
//! before anything is sent.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::obs_id::ObsId;

/// The Obs ID argument of a submit method, checked.
///
/// # Errors
///
/// `ValueError` if `obs_id` is not a valid Obs ID.
pub(super) fn job_obs_id(obs_id: u64) -> PyResult<ObsId> {
    ObsId::validate(obs_id).map_err(|e| PyValueError::new_err(e.to_string()))
}
