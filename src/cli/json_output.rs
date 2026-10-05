// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The errors of `--json`: each error is one line of JSON on stdout, the
//! MWA ASVO's `ErrorResponse` (see [`crate::asvo::error_response`]) with the
//! job ID or the obsid that it is about. With `--json`, no logs are printed.

use thiserror::Error;

use crate::asvo::apiv2::openapi::ErrorResponse;
use crate::asvo::error_response::{ERROR_CODE_CLIENT_ERROR, ERROR_CODE_INVALID_ARGUMENT};
use crate::asvo::{new_error_response, AsvoApiError, AsvoError, AsvoJobId};
use crate::obs_id::{ObsId, ObsIdError};
use crate::ParseError;

/// A command-line argument that cannot be used, found by the command (not
/// by clap or by the library). With `--json`, it is
/// [`ERROR_CODE_INVALID_ARGUMENT`].
#[derive(Error, Debug)]
#[error("{0}")]
pub(super) struct ArgumentError(pub String);

/// The failure of a run whose errors were already reported, one by one. The
/// text is the summary that the run ends with; `--json` prints nothing for
/// it, because each error already has its line, and the exit code tells
/// that the run failed.
#[derive(Error, Debug)]
#[error("{0}")]
pub(super) struct ReportedFailures(pub String);

/// One error line of `--json`: the `ErrorResponse`, then the job ID and the
/// obsid that the error is about (a key that is not known is left out).
#[derive(serde::Serialize)]
pub(super) struct JsonError {
    #[serde(flatten)]
    error: ErrorResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<AsvoJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    obs_id: Option<ObsId>,
}

impl JsonError {
    /// The error line of `error`. The first error of the library in its
    /// chain gives the `ErrorResponse`, and the job ID and the obsid when
    /// the error names them. Any other error is
    /// [`ERROR_CODE_CLIENT_ERROR`], with the whole chain as the message.
    pub(super) fn new(error: &anyhow::Error) -> Self {
        for cause in error.chain() {
            if let Some(e) = cause.downcast_ref::<AsvoError>() {
                return Self::from_response(e.error_response(), e.job_id(), e.obs_id());
            }
            if let Some(e) = cause.downcast_ref::<AsvoApiError>() {
                return Self::from_response(e.error_response(), None, None);
            }
            if let Some(e) = cause.downcast_ref::<ParseError>() {
                return Self::from_response(e.error_response(), None, None);
            }
            if let Some(e) = cause.downcast_ref::<ObsIdError>() {
                return Self::from_response(e.error_response(), None, None);
            }
            if let Some(e) = cause.downcast_ref::<ArgumentError>() {
                return Self::from_response(
                    new_error_response(ERROR_CODE_INVALID_ARGUMENT, e),
                    None,
                    None,
                );
            }
        }
        Self::from_response(
            new_error_response(ERROR_CODE_CLIENT_ERROR, format!("{error:#}")),
            None,
            None,
        )
    }

    /// The error line of a command-line usage error found by clap. The
    /// message is clap's text, without styles.
    pub(super) fn usage(error: &clap::Error) -> Self {
        Self::from_response(
            new_error_response(ERROR_CODE_INVALID_ARGUMENT, error.render()),
            None,
            None,
        )
    }

    fn from_response(
        error: ErrorResponse,
        job_id: Option<AsvoJobId>,
        obs_id: Option<ObsId>,
    ) -> Self {
        Self {
            error,
            job_id,
            obs_id,
        }
    }

    /// The line for the job ID `job_id`, which was asked for.
    pub(super) fn with_job_id(mut self, job_id: AsvoJobId) -> Self {
        self.job_id = Some(job_id);
        self
    }

    /// The line for the obsid `obs_id`, which was asked for.
    pub(super) fn with_obs_id(mut self, obs_id: ObsId) -> Self {
        self.obs_id = Some(obs_id);
        self
    }
}
