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

pub mod params;

#[cfg(test)]
mod test;

use clap::{ArgAction, Parser};

use crate::asvo::{AsvoJobState, AsvoJobType};
use params::{
    BeamformerJobArgs, ConversionJobArgs, DownloadJobArgs, ImagingFromJobArgs, ImagingJobArgs,
    VoltageJobArgs,
};

const ABOUT: &str = r#"An alternative, efficient and easy-to-use MWA ASVO client.
Source:   https://github.com/MWATelescope/giant-squid
MWA ASVO: https://asvo.mwatelescope.org"#;

#[derive(Parser, Debug)]
#[command(author, about = ABOUT, version)]
pub enum Args {
    /// List your current and recent MWA ASVO jobs
    #[command(alias = "l")]
    List {
        /// Print the jobs as a simple JSON
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// show only jobs matching the provided states, case insensitive.
        /// Options: queued, waitcal, staging, staged, retrieving, preprocessing, imaging, delivering, ready, error, expired, cancelled
        #[arg(long, id = "STATE", value_delimiter = ',')]
        states: Vec<AsvoJobState>,

        /// filter job list by type, case insensitive with underscores. Options:
        /// conversion, download_visibilities, download_metadata,
        /// download_voltage or cancel_job
        #[arg(long, id = "TYPE", value_delimiter = ',')]
        types: Vec<AsvoJobType>,

        /// Disables colouring of output. Useful when you have a non-black terminal background for example
        #[arg(short, long)]
        no_colour: bool,

        /// Only fetch jobs from the past N days. If not given, fetches your
        /// full job history.
        #[arg(long)]
        days: Option<i64>,

        /// job IDs or obsids to filter by. Files containing job IDs or
        /// obsids are also accepted.
        #[arg(id = "JOBID_OR_OBSID")]
        jobids_or_obsids: Vec<String>,
    },

    /// Download an MWA ASVO job
    #[command(alias = "d")]
    Download {
        /// Which dir should downloads be written to.
        #[arg(short, long, default_value = ".")]
        download_dir: String,

        /// Acacia delivery jobs only: Don't untar the contents of your download. NOTE: This option allows resuming downloads by rerunning giant-squid after an interruption. Giant-squid will resume where it left off.
        #[arg(short, long, visible_alias("keep-zip"))]
        keep_tar: bool,

        /// Do not attempt to resume a partial download. Leave the partial file alone.
        #[arg(short = 'r', long)]
        no_resume: bool,

        /// Download up to this number of jobs concurrently. 2-4 is a good number for most users. Set this to 0 to use the number of CPU cores you machine has
        #[arg(short = 'c', long, default_value = "4")]
        concurrent_downloads: usize,

        /// Don't verify the downloaded contents against the upstream hash.
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
        #[arg(id = "JOBID_OR_OBSID")]
        jobids_or_obsids: Vec<String>,
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit MWA ASVO preprocessing/conversion jobs
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-dec -26.7" as an unknown "-2" flag.
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit MWA ASVO imaging jobs
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-dec -26.7" as an unknown "-2" flag.
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit MWA ASVO imaging jobs from an existing conversion job.
    /// Unlike submit-image, this skips the conversion step and images
    /// directly from the output of a previous conversion job.
    // Declination and robustness are legitimately negative, and without
    // this clap reads "--custom-dec -26.7" as an unknown "-2" flag.
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA voltages
    #[command(alias = "st")]
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
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
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Wait for MWA ASVO jobs to complete, return the urls
    #[command(alias = "w")]
    Wait {
        /// Print the jobs as a simple JSON after waiting
        #[arg(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// Disables colouring of output. Useful when you have a non-black terminal background for example
        #[arg(short, long)]
        no_colour: bool,

        /// The jobs to wait for. Files containing jobs are also
        /// accepted.
        #[arg(id = "JOB")]
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

        /// The jobs to be cancelled. Files containing obsids are also
        /// accepted.
        #[arg(id = "JOB")]
        jobs: Vec<String>,
    },
}
