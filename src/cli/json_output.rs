// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The errors of `--json`: each error is one line of JSON on stdout, the
//! MWA ASVO's `ErrorResponse` (see [`crate::mwa_asvo::error_response`]) with the
//! Job ID or the Obs ID that it is about, and `"status": "failed"`. With
//! `--json`, no logs are printed.
//!
//! Every line that reports a result (a reply of the MWA ASVO, a download, an
//! error) has `status` as its first key, so a script can read it first.

use serde::ser::{Error as _, SerializeMap};
use thiserror::Error;

use crate::mwa_asvo::api::openapi::{ErrorResponse, Status};
use crate::mwa_asvo::error_response::{ERROR_CODE_CLIENT_ERROR, ERROR_CODE_INVALID_ARGUMENT};
use crate::mwa_asvo::{new_error_response, AsvoApiError, AsvoError, AsvoJobId};
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

/// One error line of `--json`: `"status": "failed"` (the schema's
/// [`Status::Failed`], the value that the MWA ASVO's own failed replies have),
/// the `ErrorResponse`, then
/// the Job ID and the Obs ID that the error is about (a key that is not known
/// is left out).
#[derive(serde::Serialize)]
pub(super) struct JsonError {
    status: Status,
    #[serde(flatten)]
    error: ErrorResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<AsvoJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    obs_id: Option<ObsId>,
}

impl JsonError {
    /// The error line of `error`. The first error of the library in its
    /// chain gives the `ErrorResponse`, and the Job ID and the Obs ID when
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

    /// The error line of an error of an API request, which is borrowed: the
    /// `ErrorResponse` of the library for it.
    pub(super) fn api_error(error: &AsvoApiError) -> Self {
        Self::from_response(error.error_response(), None, None)
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
            status: Status::Failed,
            error,
            job_id,
            obs_id,
        }
    }

    /// The line for the Job ID `job_id`, which was asked for.
    pub(super) fn with_job_id(mut self, job_id: AsvoJobId) -> Self {
        self.job_id = Some(job_id);
        self
    }

    /// The line for the Obs ID `obs_id`, which was asked for.
    pub(super) fn with_obs_id(mut self, obs_id: ObsId) -> Self {
        self.obs_id = Some(obs_id);
        self
    }
}

/// A reply of the MWA ASVO (an object with a `status` key) as a line of
/// `--json`, with `status` first and the other keys as they are. The reply
/// types of the schema have their keys in alphabetical order, which puts
/// `status` last.
pub(super) struct StatusFirst<'a, T>(pub &'a T);

impl<T: serde::Serialize> serde::Serialize for StatusFirst<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let serde_json::Value::Object(mut keys) =
            serde_json::to_value(self.0).map_err(S::Error::custom)?
        else {
            return Err(S::Error::custom(
                "a line with a status must be a JSON object",
            ));
        };
        let mut line = serializer.serialize_map(Some(keys.len()))?;
        if let Some(status) = keys.remove("status") {
            line.serialize_entry("status", &status)?;
        }
        for (key, value) in &keys {
            line.serialize_entry(key, value)?;
        }
        line.end()
    }
}
