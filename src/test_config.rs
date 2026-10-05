// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Builds an [`AsvoClientConfig`] that points at a [`TestEnv`]'s mock
//! server, for the unit tests.
//!
//! This is separate from `src/test_common.rs` because that file is also
//! compiled into `tests/cli.rs` and so must use no `crate::` items.

use crate::asvo::apiv2::openapi::JobDetailResponse;
use crate::asvo::{AsvoClientConfig, AsvoJob, JobType};
use crate::obs_id::ObsId;
use crate::test_common::{TestEnv, TEST_API_TIMEOUT};

/// A client config for the mock server in `env`, with the token cache in
/// its temporary home directory.
pub fn client_config(env: &TestEnv) -> AsvoClientConfig {
    let mut config = AsvoClientConfig::new(env.base_url(), env.api_key());
    config.api_timeout = TEST_API_TIMEOUT;
    config.token_cache_path = Some(env.token_cache_path());
    config
}

/// The job type with the schema name `name` (for example `visibility`).
pub fn job_type(name: &str) -> JobType {
    JobType::parse_name(name).expect("a test job type should be a schema name")
}

/// A job for the tests: `detail`, with `obs_id` put in its `job_params`
/// (where the API has it), checked as a job from the API is.
pub fn asvo_job(obs_id: ObsId, mut detail: JobDetailResponse) -> AsvoJob {
    detail
        .job_params
        .insert("obs_id".to_string(), serde_json::json!(u64::from(obs_id)));
    AsvoJob::try_from(detail).expect("a test job should be usable")
}
