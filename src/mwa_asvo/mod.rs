// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Code to interface with the MWA ASVO.
//!
//! Only [`api`] (the OpenAPI schema's types, the job arguments and their
//! checks) and [`error_response`] are public here. Everything else is
//! public at the crate root, and only there.
// `#[macro_use]`: see the declaration of this module in lib.rs.
#[macro_use]
pub mod api;
pub(crate) mod download;
pub(crate) mod env;
pub(crate) mod error;
pub mod error_response;
pub(crate) mod job_fields;
pub(crate) mod manta_ray_csv;
pub(crate) mod redact;
pub(crate) mod retry;
pub(crate) mod token_store;
pub(crate) mod types;
pub(crate) mod wait;

// The paths that the rest of the crate uses. Some of them only the CLI (the
// "bin" feature) or the Python module (the "python" feature) uses, so a
// build without that feature does not use them.
#[allow(unused_imports)]
pub(crate) use self::{
    api::client::{
        JobsFilter, MwaAsvoClient, MwaAsvoClientConfig, ObsIdJobParams, SubmitManyResult,
        DEFAULT_API_TIMEOUT, DEFAULT_MWA_ASVO_HOST, ENDPOINT_DOWNLOAD_JOB, ENDPOINT_IMAGE_FROM_JOB,
        ENDPOINT_JOBS, ENDPOINT_PREPROCESS_JOB,
    },
    api::openapi::{DeliveryDestination, JobFile, JobProduct, JobState, JobType, SortBy},
    api::MwaAsvoApiError,
    download::{
        DownloadManyOptions, DownloadManyResult, DownloadOptions, DownloadProgress, DownloadTarget,
        BYTES_PER_MIB, DEFAULT_CONCURRENT_DOWNLOADS, DEFAULT_DOWNLOAD_BUFFER_SIZE,
        DEFAULT_DOWNLOAD_RETRY_DURATION, MIN_DOWNLOAD_BUFFER_SIZE,
    },
    env::{
        client_config_from_env, DownloadSettings, ENV_GIANT_SQUID_DELIVERY,
        ENV_GIANT_SQUID_DELIVERY_FORMAT,
    },
    error::MwaAsvoError,
    error_response::{new_error_response, ClientErrorResponse},
    job_fields::{job_field_names, job_field_value, parse_job_field},
    token_store::default_token_cache_path,
    types::{MwaAsvoJob, MwaAsvoJobId, MwaAsvoJobVec},
    wait::{
        WaitOptions, DEFAULT_WAIT_INITIAL_DELAY, DEFAULT_WAIT_POLL_INTERVAL,
        DEFAULT_WAIT_RETRY_DURATION,
    },
};
