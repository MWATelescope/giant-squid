// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
//!
//! Only [`api`] (the OpenAPI schema's types, the job arguments and their
//! checks) and [`error_response`] are public here. Everything else is
//! public at the crate root, and only there.
pub mod api;
pub(crate) mod download;
pub(crate) mod env;
pub(crate) mod error;
pub mod error_response;
pub(crate) mod token_store;
pub(crate) mod types;

// The paths that the rest of the crate uses. Some of them only the CLI (the
// "bin" feature) or the Python module (the "python" feature) uses, so a
// build without that feature does not use them.
#[allow(unused_imports)]
pub(crate) use api::client::{
    AsvoClient, AsvoClientConfig, JobQuery, JobsFilter, DEFAULT_API_TIMEOUT, DEFAULT_ASVO_HOST,
    ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGE_FROM_JOB, ENDPOINT_IMAGING_JOB, ENDPOINT_JOBS, ENDPOINT_VOLTAGE_JOB,
};
#[allow(unused_imports)]
pub(crate) use api::openapi::{Delivery, JobFile, JobProduct, JobState, JobType};
#[allow(unused_imports)]
pub(crate) use api::AsvoApiError;
#[allow(unused_imports)]
pub(crate) use download::{
    DownloadOptions, DownloadProgress, BYTES_PER_MIB, DEFAULT_DOWNLOAD_BUFFER_SIZE,
    DEFAULT_DOWNLOAD_RETRY_DURATION,
};
#[allow(unused_imports)]
pub(crate) use env::{
    client_config_from_env, DownloadSettings, ENV_GIANT_SQUID_DELIVERY,
    ENV_GIANT_SQUID_DELIVERY_FORMAT,
};
#[allow(unused_imports)]
pub(crate) use error::AsvoError;
#[allow(unused_imports)]
pub(crate) use error_response::new_error_response;
#[allow(unused_imports)]
pub(crate) use token_store::default_token_cache_path;
#[allow(unused_imports)]
pub(crate) use types::{AsvoJob, AsvoJobId, AsvoJobVec};
