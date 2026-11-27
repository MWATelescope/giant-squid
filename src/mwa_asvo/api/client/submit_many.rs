// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The submission of one job for each of several Obs IDs:
//! [`MwaAsvoClient::submit_many`].

use super::MwaAsvoClient;
use crate::mwa_asvo::api::error::MwaAsvoApiError;
use crate::mwa_asvo::api::job_params::PreprocessJobParams;
use crate::mwa_asvo::api::openapi::{DownloadJobParams, JobSubmittedResponse};
use crate::obs_id::ObsId;

/// A request body of a job for one Obs ID, which
/// [`MwaAsvoClient::submit_many`] submits: a download body or a
/// preprocessing body. (An imaging body from a conversion job has no Obs ID.)
pub trait ObsIdJobParams {
    /// Submit the body with `client`, to the endpoint of its kind of job
    /// ([`MwaAsvoClient::submit_download_job`] or
    /// [`MwaAsvoClient::submit_preprocess_job`]).
    ///
    /// # Errors
    ///
    /// The error of that submit method.
    fn submit_with(&self, client: &MwaAsvoClient) -> Result<JobSubmittedResponse, MwaAsvoApiError>;

    /// A copy of the body for the Obs ID `obs_id`. The other fields do not
    /// change.
    #[must_use]
    fn with_obs_id(&self, obs_id: ObsId) -> Self;
}

impl ObsIdJobParams for DownloadJobParams {
    fn submit_with(&self, client: &MwaAsvoClient) -> Result<JobSubmittedResponse, MwaAsvoApiError> {
        client.submit_download_job(self)
    }

    fn with_obs_id(&self, obs_id: ObsId) -> Self {
        Self {
            obs_id: i64::from(obs_id),
            ..self.clone()
        }
    }
}

impl ObsIdJobParams for PreprocessJobParams {
    fn submit_with(&self, client: &MwaAsvoClient) -> Result<JobSubmittedResponse, MwaAsvoApiError> {
        client.submit_preprocess_job(self)
    }

    fn with_obs_id(&self, obs_id: ObsId) -> Self {
        let obs_id = i64::from(obs_id);
        match self {
            Self::Conversion(params) => {
                let mut params = params.clone();
                params.obs_id = obs_id;
                Self::Conversion(params)
            }
            Self::Imaging(params) => {
                let mut params = params.clone();
                params.obs_id = obs_id;
                Self::Imaging(params)
            }
        }
    }
}

/// One result of [`MwaAsvoClient::submit_many`]: the Obs ID, and the reply
/// of the MWA ASVO or the error of the submission.
pub type SubmitManyResult = (ObsId, Result<JobSubmittedResponse, MwaAsvoApiError>);

impl MwaAsvoClient {
    /// Submit one job for each of `obs_ids`, in their order, as the
    /// `giant-squid submit-*` commands do. `build` makes the request body of
    /// each Obs ID (for example `|obs_id| params.with_obs_id(obs_id)`).
    ///
    /// A submission that fails does not stop the others: every Obs ID is
    /// submitted. An Obs ID that is given more than once is submitted each
    /// time. `on_result` is called with each result when its request is
    /// done, so that a caller can report it at once.
    ///
    /// Returns the result of each Obs ID, in the order of `obs_ids`. A body
    /// with a value outside the schema's limits is not sent: its result is
    /// [`MwaAsvoApiError::InvalidParameter`].
    pub fn submit_many<P: ObsIdJobParams>(
        &self,
        obs_ids: &[ObsId],
        build: impl Fn(ObsId) -> P,
        mut on_result: impl FnMut(ObsId, &Result<JobSubmittedResponse, MwaAsvoApiError>),
    ) -> Vec<SubmitManyResult> {
        obs_ids
            .iter()
            .map(|&obs_id| {
                let result = build(obs_id).submit_with(self);
                on_result(obs_id, &result);
                (obs_id, result)
            })
            .collect()
    }
}
