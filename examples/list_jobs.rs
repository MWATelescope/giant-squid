// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! List your MWA ASVO jobs with the giant-squid library only (no CLI code).
//!
//! The library reads no environment variables, so this program (the
//! caller) reads them and builds an `AsvoClientConfig`:
//!
//! ```text
//! MWA_ASVO_API_KEY=<your key> cargo run --example list_jobs [DAYS]
//! ```
//!
//! Set `MWA_ASVO_HOST` to use a server other than the production MWA ASVO,
//! for example `https://test-asvo.mwatelescope.org`. If `HOME` is set, the
//! session is cached in the token file that giant-squid and mwa-cli share.
//! `DAYS` (optional) lists only the jobs from the past `DAYS` days.
//!
//! This example builds without the `bin` feature:
//!
//! ```text
//! cargo run --no-default-features --example list_jobs
//! ```

use std::env;
use std::error::Error;
use std::path::Path;

use mwa_giant_squid::{
    default_token_cache_path, AsvoClient, AsvoClientConfig, JobState, JobsFilter, DEFAULT_ASVO_HOST,
};

/// The environment variable that holds the API key.
const ENV_API_KEY: &str = "MWA_ASVO_API_KEY";
/// The environment variable that overrides the MWA ASVO host.
const ENV_HOST: &str = "MWA_ASVO_HOST";
/// The environment variable that holds the home directory.
const ENV_HOME: &str = "HOME";

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = env::var(ENV_API_KEY).map_err(|_| format!("{ENV_API_KEY} is not set"))?;
    let host = env::var(ENV_HOST).unwrap_or_else(|_| DEFAULT_ASVO_HOST.to_string());
    let days: Option<i64> = env::args().nth(1).map(|d| d.parse()).transpose()?;

    let mut config = AsvoClientConfig::new(host, api_key);
    config.token_cache_path = env::var(ENV_HOME)
        .ok()
        .map(|home| default_token_cache_path(Path::new(&home)));

    let client = AsvoClient::new(config)?;
    let jobs = client.get_jobs(&JobsFilter {
        days,
        ..JobsFilter::default()
    })?;

    if jobs.0.is_empty() {
        println!("You have no jobs.");
        return Ok(());
    }

    println!("{:>10}  {:>10}  {:<24}  State", "Job ID", "Obsid", "Type");
    for job in &jobs.0 {
        println!(
            "{:>10}  {:>10}  {:<24}  {}",
            job.job_id,
            job.obs_id,
            job.job_type.map(|t| t.name()).unwrap_or_default(),
            job.job_state
        );
    }

    // An example of using the library's filters: only the ready jobs.
    let ready = jobs.filter(&[], &[], &[], &[JobState::Completed]);
    println!("{} of the jobs are ready for download.", ready.0.len());

    Ok(())
}
