// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
pub mod api;
mod download;
mod env;
mod error;
pub mod error_response;
mod token_store;
mod types;

pub use api::client::{
    AsvoClient, AsvoClientConfig, JobQuery, JobsFilter, DEFAULT_API_TIMEOUT,
    ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGE_FROM_JOB, ENDPOINT_IMAGING_JOB, ENDPOINT_JOBS, ENDPOINT_VOLTAGE_JOB,
};
pub use api::openapi::{Delivery, JobFile, JobProduct, JobState, JobType};
pub use api::AsvoApiError;
pub use download::{
    DownloadOptions, DownloadProgress, BYTES_PER_MIB, DEFAULT_CONCURRENT_DOWNLOADS,
    DEFAULT_DOWNLOAD_BUFFER_SIZE, DEFAULT_DOWNLOAD_RETRY_DURATION,
};
pub use env::{
    client_config_from_env, DownloadSettings, ENV_GIANT_SQUID_BUF_SIZE, ENV_GIANT_SQUID_DELIVERY,
    ENV_GIANT_SQUID_DELIVERY_FORMAT, ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, ENV_HOME,
    ENV_MWA_ASVO_API_KEY, ENV_MWA_ASVO_API_TIMEOUT, ENV_MWA_ASVO_HOST,
};
pub use error::AsvoError;
pub use error_response::new_error_response;
pub use token_store::{default_token_cache_path, StoredTokens};
pub use types::{AsvoJob, AsvoJobId, AsvoJobMap, AsvoJobVec};

use std::time::Duration;

/// The production MWA ASVO host. Callers that do not need a different
/// server (for example a test or development instance) use this as
/// [`AsvoClientConfig::host`].
pub const DEFAULT_ASVO_HOST: &str = "https://asvo.mwatelescope.org:443";

/// The time between two job list requests while waiting for jobs, in the
/// `giant-squid` commands.
pub const WAIT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// How long the `giant-squid` commands wait before the first job list request
/// of a wait, so that the user's queue is hopefully current.
pub const WAIT_INITIAL_DELAY: Duration = Duration::from_secs(1);
