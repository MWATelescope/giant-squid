// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The giant-squid command line interface.
//!
//! The clap definition lives in the library rather than in `src/bin` so that
//! it can be exercised directly by tests: every invocation form can be
//! parsed with `Args::try_parse_from` and the resulting request body checked
//! without contacting an MWA ASVO server. The binary is left with dispatch
//! and I/O only.

pub mod legacy_json;
pub mod params;
pub mod run;
pub mod table;
pub mod value_enums;

#[cfg(test)]
mod tests;

use clap::{ArgAction, Parser};
use jiff::Timestamp;

use crate::asvo::apiv2::validate;
use crate::asvo::DEFAULT_CONCURRENT_DOWNLOADS;
use crate::asvo::{AsvoJobState, AsvoJobType};
use params::{
    list_days_default, parse_i64_bounds, parse_utc_time, BeamformerJobArgs, ConversionJobArgs,
    DownloadJobArgs, ImagingFromJobArgs, ImagingJobArgs, VoltageJobArgs,
};

/// The name of the program, which `--version` prints. Without it, clap
/// prints the name of the crate (`mwa_giant_squid`).
pub const PROGRAM_NAME: &str = "giant-squid";

/// The help of `list --job-states`, before the list of states.
const JOB_STATES_HELP: &str = "show only jobs matching the provided states, case insensitive.";

/// The help of `list --job-types`, before the list of types.
const JOB_TYPES_HELP: &str = "filter job list by type, case insensitive with underscores.";

/// The help of `list --job-states`: the text above and the states that the
/// library accepts, so that the help cannot differ from the parser.
fn job_states_help() -> String {
    format!(
        "{JOB_STATES_HELP} Options: {}",
        AsvoJobState::names().join(", ")
    )
}

/// The help of `list --job-types`: the text above and the job types that the
/// library accepts.
fn job_types_help() -> String {
    format!(
        "{JOB_TYPES_HELP} Options: {}",
        AsvoJobType::names().join(", ")
    )
}

const ABOUT: &str = r#"An alternative, efficient and easy-to-use MWA ASVO client.
Source:   https://github.com/MWATelescope/giant-squid
MWA ASVO: https://asvo.mwatelescope.org"#;

#[derive(Parser, Debug)]
#[command(name = PROGRAM_NAME, author, about = ABOUT, version)]
pub enum Args {
    /// List your current and recent MWA ASVO jobs
    #[command(alias = "l")]
    List {
        /// Print the jobs as a simple JSON
        #[arg(short, long)]
        json: bool,

        /// Print the jobs as JSON in the old format of giant-squid before
        /// 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl,
        /// ...). Deprecated: this option will be removed in the release
        /// after 3.0.0. Use --json.
        #[arg(long, conflicts_with = "json")]
        legacy_json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        // The help is built from the library's names (see `job_states_help`).
        #[arg(long, id = "JOB_STATE", alias = "states", value_delimiter = ',', help = job_states_help())]
        job_states: Vec<AsvoJobState>,

        // The help is built from the library's names (see `job_types_help`).
        #[arg(long, id = "JOB_TYPE", alias = "types", value_delimiter = ',', help = job_types_help())]
        job_types: Vec<AsvoJobType>,

        /// Disables colouring of output. Useful when you have a non-black terminal background for example
        #[arg(short, long)]
        no_colour: bool,

        /// Only fetch jobs from the past N days (1 to 30)
        #[arg(long, default_value = list_days_default().to_string(), value_parser = parse_i64_bounds(validate::DAYS))]
        days: Option<i64>,

        /// Only jobs created at or after this time: RFC 3339 (for example
        /// 2026-09-01T00:00:00Z) or a date (2026-09-01, midnight UTC).
        #[arg(long, value_parser = parse_utc_time)]
        date_from: Option<Timestamp>,

        /// Only jobs created at or before this time: RFC 3339 or a date
        /// (midnight UTC).
        #[arg(long, value_parser = parse_utc_time)]
        date_to: Option<Timestamp>,

        /// The column to sort the jobs by, for example "id".
        #[arg(long)]
        sort_by: Option<String>,

        /// job IDs or obsids to filter by. Files containing job IDs or
        /// obsids are also accepted.
        #[arg(id = "JOB_ID_OR_OBS_ID")]
        job_ids_or_obs_ids: Vec<String>,
    },

    /// Download an MWA ASVO job
    #[command(alias = "d")]
    Download {
        /// Which dir should downloads be written to.
        #[arg(short, long, default_value = ".")]
        download_dir: String,

        /// Acacia delivery jobs only: Don't untar the contents of your download.
        #[arg(short, long, visible_alias("keep-zip"))]
        keep_tar: bool,

        /// Do not resume a partial download: download it again from the start. A complete keep-tar file that matches the MWA ASVO hash is still skipped. Without this option, a rerun after an interruption carries on where it stopped, with or without --keep-tar.
        #[arg(short = 'r', long)]
        no_resume: bool,

        /// Download up to this number of jobs concurrently. 2-4 is a good number for most users. Set this to 0 to use the number of CPU cores you machine has
        #[arg(short = 'c', long, default_value_t = DEFAULT_CONCURRENT_DOWNLOADS)]
        concurrent_downloads: usize,

        /// Don't verify the downloaded contents against the upstream hash. The hash is still checked when a stream-untar download uses files from an earlier run, and when a complete --keep-tar file is already on disk.
        #[arg(long)]
        skip_hash: bool,

        // Does nothing: hash check is enabled by default. This is for backwards compatibility.
        #[arg(long, hide = true)]
        hash: bool,

        /// Don't actually download; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The job IDs or obsids to be downloaded. Files containing job IDs or
        /// obsids are also accepted.
        #[arg(id = "JOB_ID_OR_OBS_ID")]
        job_ids_or_obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA raw visibilities
    #[command(alias = "sv")]
    SubmitVis {
        #[command(flatten)]
        download: DownloadJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO preprocessing/conversion jobs
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-centre-dec -26.7" as an unknown "-2" flag.
    #[command(alias = "sc", allow_negative_numbers = true)]
    SubmitConv {
        #[command(flatten)]
        conv: ConversionJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO imaging jobs
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-centre-dec -26.7" as an unknown "-2" flag.
    #[command(alias = "si", allow_negative_numbers = true)]
    SubmitImage {
        #[command(flatten)]
        image: ImagingJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to submit for imaging. Files containing obsids are
        /// also accepted. All obsids in one invocation share the same
        /// parameters above.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO imaging jobs from an existing conversion job.
    /// Unlike submit-image, this skips the conversion step and images
    /// directly from the output of a previous conversion job.
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-centre-dec -26.7" as an unknown "-2" flag.
    #[command(alias = "sifj", allow_negative_numbers = true)]
    SubmitImageFromJob {
        #[command(flatten)]
        image: ImagingFromJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsid to image. Exactly one obsid is required (the
        /// source_job_id identifies the conversion job for this obsid).
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA metadata — metafits (with PPDs
    /// for each tile) and RFI flags (if available)
    #[command(alias = "sm")]
    SubmitMeta {
        #[command(flatten)]
        download: DownloadJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA voltages
    // Without this, clap reads "--offset -1" as an unknown "-1" flag, not
    // as a value, and the offset's range check is never reached.
    #[command(alias = "st", allow_negative_numbers = true)]
    SubmitVolt {
        #[command(flatten)]
        volt: VoltageJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA beamformer files (vdif,hdr,fil)
    #[command(alias = "sb")]
    SubmitBf {
        #[command(flatten)]
        bf: BeamformerJobArgs,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Print each submitted job's response from the MWA ASVO as one line
        /// of JSON on stdout.
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBS_ID")]
        obs_ids: Vec<String>,
    },

    /// Wait for MWA ASVO jobs to complete, return the urls
    #[command(alias = "w")]
    Wait {
        /// Print the jobs as a simple JSON after waiting
        #[arg(short, long)]
        json: bool,

        /// Print the jobs as JSON in the old format of giant-squid before
        /// 3.0.0 (camelCase keys: obsid, jobId, jobType, jobState, fileUrl,
        /// ...). Deprecated: this option will be removed in the release
        /// after 3.0.0. Use --json.
        #[arg(long, conflicts_with = "json")]
        legacy_json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// Disables colouring of output. Useful when you have a non-black terminal background for example
        #[arg(short, long)]
        no_colour: bool,

        /// The job IDs to wait for. Files containing job IDs are also
        /// accepted.
        #[arg(id = "JOB_ID")]
        jobs: Vec<String>,
    },

    /// Cancel MWA ASVO job
    #[command(alias = "c")]
    Cancel {
        /// Don't actually cancel; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The job IDs to be cancelled. Files containing job IDs are also
        /// accepted.
        #[arg(id = "JOB_ID")]
        jobs: Vec<String>,
    },
}
