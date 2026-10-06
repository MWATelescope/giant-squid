// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An alternative, efficient and easy-to-use interface for the MWA ASVO.

pub mod mwa_asvo;
// The CLI definition needs clap, which only the binary feature pulls in. It
// lives here rather than in src/bin so that tests can parse argument
// vectors directly - see docs/TESTING.md.
#[cfg(feature = "bin")]
pub mod cli;
mod helpers;
mod obs_id;
// The Python module. Built by maturin; see pyproject.toml.
#[cfg(feature = "python")]
mod python;
/// Collects the Python stub information for the `stub_gen` binary.
#[cfg(feature = "python-stubgen")]
#[doc(hidden)]
pub use python::stub_info;
#[cfg(test)]
mod test_common;
#[cfg(test)]
mod test_config;

// The public API: each item has one path. The OpenAPI schema's types, the
// job arguments and their checks are in `mwa_asvo::api`, and the error codes
// of `--json` in `mwa_asvo::error_response`; everything else is here.
pub use helpers::{
    check_file_sha1_hash, parse_job_ids_and_obs_ids_from_file, parse_job_ids_only,
    parse_many_job_ids_or_obs_ids, parse_obs_ids_only, parse_utc_time, ParseError, OBS_ID_HINT,
};
/// The jiff crate, for the dates and times in this crate's API (for example
/// the `created` field of a job), so that a program uses the same jiff version.
pub use jiff;
pub use mwa_asvo::api::client::{
    AsvoClient, AsvoClientConfig, JobQuery, JobsFilter, API_PREFIX, DEFAULT_API_TIMEOUT,
    DEFAULT_ASVO_HOST, ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGE_FROM_JOB, ENDPOINT_IMAGING_JOB, ENDPOINT_JOBS, ENDPOINT_VOLTAGE_JOB,
};
pub use mwa_asvo::api::error::AsvoApiError;
pub use mwa_asvo::download::{
    DownloadOptions, DownloadProgress, BYTES_PER_MIB, DEFAULT_DOWNLOAD_BUFFER_SIZE,
    DEFAULT_DOWNLOAD_RETRY_DURATION,
};
pub use mwa_asvo::env::{
    client_config_from_env, DownloadSettings, ENV_GIANT_SQUID_BUF_SIZE, ENV_GIANT_SQUID_DELIVERY,
    ENV_GIANT_SQUID_DELIVERY_FORMAT, ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, ENV_HOME,
    ENV_MWA_ASVO_API_KEY, ENV_MWA_ASVO_API_TIMEOUT, ENV_MWA_ASVO_HOST,
};
pub use mwa_asvo::error::AsvoError;
pub use mwa_asvo::token_store::{default_token_cache_path, StoredTokens};
pub use mwa_asvo::types::{AsvoJob, AsvoJobId, AsvoJobVec};
pub use obs_id::{ObsId, ObsIdError};
