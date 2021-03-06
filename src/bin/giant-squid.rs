// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::bail;
use log::{debug, info};
use simplelog::*;
use structopt::{clap::AppSettings, StructOpt};

use mwa_giant_squid::asvo::*;
use mwa_giant_squid::*;

const ABOUT: &str = r#"An alternative, efficient and easy-to-use MWA ASVO client.
Source:   https://github.com/MWATelescope/giant-squid
MWA ASVO: https://asvo.mwatelescope.org"#;

lazy_static::lazy_static! {
    static ref DEFAULT_CONVERSION_PARAMETERS_TEXT: String = {
        let mut s = "The cotter parameters used. If any of the default parameters are not overwritten, then they remain. Default: ".to_string();
        for (i, (k, v)) in DEFAULT_CONVERSION_PARAMETERS.iter().enumerate() {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            if i != DEFAULT_CONVERSION_PARAMETERS.len() - 1 {
                s.push_str(", ");
            }
        }
        s
    };
}

#[derive(StructOpt, Debug)]
#[structopt(author, about=ABOUT,
    global_settings = &[AppSettings::ColoredHelp,
                        AppSettings::DeriveDisplayOrder])]
enum Opts {
    /// List ASVO jobs
    #[structopt(alias = "l")]
    List {
        /// Print the jobs as a simple JSON
        #[structopt(short, long)]
        json: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[structopt(short, long, parse(from_occurrences))]
        verbosity: u8,
    },

    /// Download an ASVO job
    #[structopt(alias = "d")]
    Download {
        /// Don't unzip the contents from the ASVO.
        #[structopt(short, long)]
        keep_zip: bool,

        /// Verify the downloaded contents against the upstream hash.
        #[structopt(long)]
        hash: bool,

        /// Don't actually download; print information on what would've happened
        /// instead.
        #[structopt(short = "n", long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[structopt(short, long, parse(from_occurrences))]
        verbosity: u8,

        /// The job IDs or obsids to be downloaded. Files containing job IDs or
        /// obsids are also accepted.
        #[structopt(name = "JOBID_OR_OBSID")]
        jobids_or_obsids: Vec<String>,
    },

    /// Submit ASVO jobs to download MWA raw visibilities
    #[structopt(alias = "sv")]
    SubmitVis {
        /// Tell the ASVO to let jobs expire after this many days.
        #[structopt(short, long, default_value = "7")]
        expiry_days: u8,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[structopt(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[structopt(short = "n", long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[structopt(short, long, parse(from_occurrences))]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[structopt(name = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit ASVO conversion jobs
    #[structopt(alias = "sc")]
    SubmitConv {
        #[structopt(short, long, help=&DEFAULT_CONVERSION_PARAMETERS_TEXT)]
        parameters: Option<String>,

        /// Tell the ASVO to let jobs expire after this many days.
        #[structopt(short, long, default_value = "7")]
        expiry_days: u8,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[structopt(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[structopt(short = "n", long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[structopt(short, long, parse(from_occurrences))]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[structopt(name = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit ASVO jobs to download MWA metadata (metafits and cotter flags)
    #[structopt(alias = "sm")]
    SubmitMeta {
        /// Tell the ASVO to let jobs expire after this many days.
        #[structopt(short, long, default_value = "7")]
        expiry_days: u8,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[structopt(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[structopt(short = "n", long)]
        dry_run: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[structopt(short, long, parse(from_occurrences))]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[structopt(name = "OBSID")]
        obsids: Vec<String>,
    },
}

fn init_logger(level: u8) {
    let config = ConfigBuilder::new().set_time_to_local(true).build();
    match level {
        0 => SimpleLogger::init(LevelFilter::Info, config).unwrap(),
        1 => SimpleLogger::init(LevelFilter::Debug, config).unwrap(),
        _ => SimpleLogger::init(LevelFilter::Trace, config).unwrap(),
    };
}

/// Wait for all of the specified job IDs to become ready, then exit.
fn wait_loop(client: AsvoClient, jobids: Vec<AsvoJobID>) -> Result<(), AsvoError> {
    loop {
        // Get the current state of all jobs. By converting to a map, we avoid
        // quadratic complexity below. Probably not a big deal, but why not?
        let jobs = client.get_jobs()?.into_map();
        let mut any_not_ready = false;
        // Iterate over all supplied job IDs.
        for j in &jobids {
            // Find the relevant job in the queue.
            let job = match jobs.0.get(j) {
                None => return Err(AsvoError::NoAsvoJob(*j)),
                Some(job) => job,
            };
            // Handle the job's state. If it's ready, there's nothing to do. If
            // the job is simply queued or in processing, we can say that we're
            // not ready yet; exit early. All other possibilities are handled
            // drastically.
            match &job.state {
                AsvoJobState::Ready => (),
                AsvoJobState::Error(e) => {
                    return Err(AsvoError::UpstreamError {
                        jobid: *j,
                        obsid: job.obsid,
                        error: e.to_string(),
                    })
                }
                AsvoJobState::Expired => return Err(AsvoError::Expired(*j)),
                AsvoJobState::Cancelled => return Err(AsvoError::Cancelled(*j)),
                AsvoJobState::Queued | AsvoJobState::Processing => {
                    any_not_ready = true;
                    break;
                }
            }
        }
        // Our lock variable is set if we broke out of the loop.
        if any_not_ready {
            std::thread::sleep(Duration::from_secs(60));
        } else {
            // If we reach here, all jobs are ready.
            break;
        }
    }
    info!("All {} ASVO jobs are ready for download.", jobids.len());
    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    match Opts::from_args() {
        Opts::List { verbosity, json } => {
            init_logger(verbosity);

            let client = AsvoClient::new()?;
            let jobs = client.get_jobs()?;
            if json {
                println!("{}", jobs.json()?);
            } else {
                jobs.list();
            }
        }

        Opts::Download {
            keep_zip,
            hash,
            dry_run,
            verbosity,
            jobids_or_obsids,
        } => {
            if jobids_or_obsids.is_empty() {
                bail!("No jobs specified!");
            }
            init_logger(verbosity);

            let (jobids, obsids) = parse_many_jobids_or_obsids(&jobids_or_obsids)?;

            if dry_run {
                if !jobids.is_empty() {
                    debug!("Parsed job IDs: {:#?}", jobids);
                }
                if !obsids.is_empty() {
                    debug!("Parsed obsids: {:#?}", obsids);
                }
                info!(
                    "Parsed {} jobids and {} obsids for download.",
                    jobids.len(),
                    obsids.len(),
                );
            } else {
                let client = AsvoClient::new()?;
                for j in jobids {
                    client.download_job(j, keep_zip, hash)?;
                }
                for o in obsids {
                    client.download_obsid(o, keep_zip, hash)?;
                }
            }
        }

        Opts::SubmitVis {
            expiry_days,
            wait,
            dry_run,
            verbosity,
            obsids,
        } => {
            let (parsed_jobids, parsed_obsids) = parse_many_jobids_or_obsids(&obsids)?;
            // There shouldn't be any job IDs here.
            if !parsed_jobids.is_empty() {
                bail!(
                    "Expected only obsids, but found these exceptions: {:?}",
                    parsed_jobids
                );
            }
            if parsed_obsids.is_empty() {
                bail!("No obsids specified!");
            }
            init_logger(verbosity);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for visibility download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                for o in parsed_obsids {
                    let j = client.submit_vis(o, expiry_days)?;
                    info!("Submitted {} as ASVO job ID {}", o, j);
                    jobids.push(j);
                }
                info!("Submitted {} obsids for visibility download.", obsids.len());

                if wait {
                    info!("Waiting for jobs to be ready...");
                    // Offer the ASVO a kindness by waiting a few seconds, so
                    // that the user's queue is hopefully current.
                    std::thread::sleep(Duration::from_secs(5));
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(client, jobids)?;
                }
            }
        }

        Opts::SubmitConv {
            parameters,
            expiry_days,
            wait,
            dry_run,
            verbosity,
            obsids,
        } => {
            let (parsed_jobids, parsed_obsids) = parse_many_jobids_or_obsids(&obsids)?;
            // There shouldn't be any job IDs here.
            if !parsed_jobids.is_empty() {
                bail!(
                    "Expected only obsids, but found these exceptions: {:?}",
                    parsed_jobids
                );
            }
            if parsed_obsids.is_empty() {
                bail!("No obsids specified!");
            }
            init_logger(verbosity);

            // Get the user parameters and set any defaults that the user has not set.
            let params = {
                let mut params = match &parameters {
                    Some(s) => parse_key_value_pairs(&s)?,
                    None => BTreeMap::new(),
                };
                for (&key, &value) in DEFAULT_CONVERSION_PARAMETERS.iter() {
                    if !params.contains_key(key) {
                        params.insert(key, value);
                    }
                }
                params
            };

            if dry_run {
                info!(
                    "Would have submitted {} obsids for conversion, using these parameters:\n{:?}",
                    obsids.len(),
                    params
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                for o in parsed_obsids {
                    let j = client.submit_conv(o, expiry_days, &params)?;
                    info!("Submitted {} as ASVO job ID {}", o, j);
                    jobids.push(j);
                }
                info!("Submitted {} obsids for conversion.", obsids.len());

                if wait {
                    info!("Waiting for jobs to be ready...");
                    // Offer the ASVO a kindness by waiting a few seconds, so
                    // that the user's queue is hopefully current.
                    std::thread::sleep(Duration::from_secs(5));
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(client, jobids)?;
                }
            }
        }

        Opts::SubmitMeta {
            expiry_days,
            wait,
            dry_run,
            verbosity,
            obsids,
        } => {
            let (parsed_jobids, parsed_obsids) = parse_many_jobids_or_obsids(&obsids)?;
            // There shouldn't be any job IDs here.
            if !parsed_jobids.is_empty() {
                bail!(
                    "Expected only obsids, but found these exceptions: {:?}",
                    parsed_jobids
                );
            }
            if parsed_obsids.is_empty() {
                bail!("No obsids specified!");
            }
            init_logger(verbosity);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for metadata download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                for o in parsed_obsids {
                    let j = client.submit_meta(o, expiry_days)?;
                    info!("Submitted {} as ASVO job ID {}", o, j);
                    jobids.push(j);
                }
                info!("Submitted {} obsids for metadata download.", obsids.len());

                if wait {
                    info!("Waiting for jobs to be ready...");
                    // Offer the ASVO a kindness by waiting a few seconds, so
                    // that the user's queue is hopefully current.
                    std::thread::sleep(Duration::from_secs(5));
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(client, jobids)?;
                }
            }
        }
    }

    Ok(())
}
