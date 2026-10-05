// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The obsid of the submit methods.
//!
//! The submit methods fill the library's argument struct of each job type
//! ([`crate::asvo::apiv2::job_args`]), which makes the request body, as the
//! CLI does. Every optional Python argument is `None` by default, which
//! leaves the field unset, so the body has the schema's default. The Python
//! argument names are the OpenAPI field names. An argument that the schema
//! does not allow raises `ValueError` (see `super::error::api_error`), and
//! nothing is sent.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::obs_id::ObsId;

/// The obsid argument of a submit method, checked.
///
/// # Errors
///
/// `ValueError` if `obs_id` is not a valid obsid.
pub(super) fn job_obs_id(obs_id: u64) -> PyResult<ObsId> {
    ObsId::validate(obs_id).map_err(|e| PyValueError::new_err(e.to_string()))
}
