// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Obs ID of the submit methods.
//!
//! The submit methods start from the library's request body of each job
//! type ([`crate::mwa_asvo::api::job_params`]) and set every field from
//! their arguments, as the CLI does. The default of each Python argument is
//! the schema's default, made by the same library function, so it is the
//! same in Rust, the CLI and Python. An argument that the schema allows to
//! be `null` accepts `None`, which is sent as `null`. The Python argument
//! names are the OpenAPI field names. An argument that the schema does not
//! allow raises `ValueError` (see `super::error::api_error`), and nothing
//! is sent.

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
