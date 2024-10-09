// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::bail;
use clap::{ArgAction, Parser};
use log::{debug, info};
use simplelog::*;

use mwa_giant_squid::asvo::*;
use mwa_giant_squid::*;

const ABOUT: &str = r#"An alternative, efficient and easy-to-use MWA ASVO client.
Source:   https://github.com/MWATelescope/giant-squid
MWA ASVO: https://asvo.mwatelescope.org"#;

lazy_static::lazy_static! {
    static ref DEFAULT_CONVERSION_PARAMETERS_TEXT: String = {
        let mut s = "The Birli parameters used. If any of the default parameters are not overwritten, then they remain. If the delivery option is specified here, it is ignored; delivery must be passed in as a command-line argument. Default: ".to_string();
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

#[derive(Parser, Debug)]
#[command(author, about = ABOUT, version)]
//#[arg(global_setting(AppSettings::DeriveDisplayOrder))]
enum Args {
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
        /// Options: queued, processing, ready, error, expired, cancelled.
        #[arg(long, id = "STATE", value_delimiter = ',')]
        states: Vec<AsvoJobState>,

        /// filter job list by type, case insensitive with underscores. Options:
        /// conversion, download_visibilities, download_metadata,
        /// download_voltage or cancel_job
        #[arg(long, id = "TYPE", value_delimiter = ',')]
        types: Vec<AsvoJobType>,

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

        /// Don't unzip the contents of your download from the MWA ASVO.
        #[arg(short, long)]
        keep_zip: bool,

        /// Don't verify the downloaded contents against the upstream hash.
        #[arg(long)]
        skip_hash: bool,

        // Does nothing: hash check is enabled by default. This is for backwards compatibility
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
        /// Tell the MWA ASVO where to deliver the job. The default is "acacia", but
        /// this can be overridden with the environment variable
        /// GIANT_SQUID_DELIVERY.
        #[arg(short, long)]
        delivery: Option<String>,

        /// Tell MWA ASVO to deliver the data in a particular format.
        /// Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia`
        /// which is always `tar`
        #[arg(short = 'f', long)]
        delivery_format: Option<String>,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Allow resubmit- if exact same job params already in your queue
        /// allow submission anyway. Default: allow resubmit is False / not present
        #[arg(short = 'r', long, action=ArgAction::SetTrue)]
        allow_resubmit: bool,

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
    #[command(alias = "sc")]
    SubmitConv {
        #[arg(short, long, help = DEFAULT_CONVERSION_PARAMETERS_TEXT.as_str())]
        parameters: Option<String>,

        /// Tell the MWA ASVO where to deliver the job. The default is "acacia", but
        /// this can be overridden with the environment variable
        /// GIANT_SQUID_DELIVERY.
        #[arg(short, long)]
        delivery: Option<String>,

        /// Tell MWA ASVO to deliver the data in a particular format.
        /// Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia`
        /// which is always `tar`
        #[arg(short = 'f', long)]
        delivery_format: Option<String>,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Allow resubmit- if exact same job params already in your queue
        /// allow submission anyway. Default: allow resubmit is False / not present
        #[arg(short = 'r', long, action=ArgAction::SetTrue)]
        allow_resubmit: bool,

        /// The verbosity of the program. The default is to print high-level
        /// information.
        #[arg(short, long, action=ArgAction::Count)]
        verbosity: u8,

        /// The obsids to be submitted. Files containing obsids are also
        /// accepted.
        #[arg(id = "OBSID")]
        obsids: Vec<String>,
    },

    /// Submit MWA ASVO jobs to download MWA metadata- metafits (with PPDs for each tile) and cotter flags (if available)
    #[command(alias = "sm")]
    SubmitMeta {
        /// Tell MWA ASVO where to deliver the job. The default is "acacia", but
        /// this can be overridden with the environment variable
        /// GIANT_SQUID_DELIVERY.
        #[arg(short, long)]
        delivery: Option<String>,

        /// Tell MWA ASVO to deliver the data in a particular format.
        /// Available value(s): `tar`. NOTE: this option does not apply if delivery = `acacia`
        /// which is always `tar`
        #[arg(short = 'f', long)]
        delivery_format: Option<String>,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Allow resubmit- if exact same job params already in your queue
        /// allow submission anyway. Default: allow resubmit is False / not present
        #[arg(short = 'r', long, action=ArgAction::SetTrue)]
        allow_resubmit: bool,

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
        /// Tell the MWA ASVO where to deliver the job. The only valid value for a voltage
        /// job is "scratch".
        #[arg(short, long)]
        delivery: Option<String>,

        /// The offset in seconds from the start GPS time of the observation.
        #[arg(short, long)]
        offset: i32,

        /// The duration (in seconds) to download.
        #[arg(short = 'u', long)]
        duration: i32,

        /// The 'from' receiver channel number (0-255)
        #[arg(short = 'f', long)]
        from_channel: Option<i32>,

        /// The 'to' receiver channel number (0-255)
        #[arg(short = 't', long)]
        to_channel: Option<i32>,

        /// Do not exit giant-squid until the specified obsids are ready for
        /// download.
        #[arg(short, long)]
        wait: bool,

        /// Don't actually submit; print information on what would've happened
        /// instead.
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Allow resubmit- if exact same job params already in your queue
        /// allow submission anyway. Default: allow resubmit is False / not present
        #[arg(short = 'r', long, action=ArgAction::SetTrue)]
        allow_resubmit: bool,

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

        /// The jobs to wait for. Files containing jobs are also
        /// accepted.
        #[arg(id = "JOB")]
        jobs: Vec<String>,
    },
}

fn init_logger(level: u8) {
    let config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in SimpleLogger")
        .build();
    match level {
        0 => SimpleLogger::init(LevelFilter::Info, config).unwrap(),
        1 => SimpleLogger::init(LevelFilter::Debug, config).unwrap(),
        _ => SimpleLogger::init(LevelFilter::Trace, config).unwrap(),
    };
}

/// Wait for all of the specified job IDs to become ready, then exit.
fn wait_loop(client: &AsvoClient, jobids: &[AsvoJobID]) -> Result<(), AsvoError> {
    info!("Waiting for {} jobs to be ready...", jobids.len());
    let mut last_state = BTreeMap::<AsvoJobID, AsvoJobState>::new();
    // Offer the ASVO a kindness by waiting a few seconds, so
    // that the user's queue is hopefully current.
    std::thread::sleep(Duration::from_secs(5));
    loop {
        // Get the current state of all jobs. By converting to a map, we avoid
        // quadratic complexity below. Probably not a big deal, but why not?
        let jobs = client.get_jobs()?.into_map();
        let mut any_not_ready = false;
        // Iterate over all supplied job IDs.
        for j in jobids {
            // Find the relevant job in the queue.
            let job = match jobs.0.get(j) {
                None => return Err(AsvoError::NoAsvoJob(*j)),
                Some(job) => job,
            };
            // Handle the job's state. If it's ready, there's nothing to do. If
            // the job is simply queued or in processing, we can say that we're
            // not ready yet. All other possibilities are handled drastically.
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
                }
            }
            // log if there was a change in state.
            match last_state.insert(*j, job.state.clone()) {
                Some(last_state) if last_state != job.state => {
                    info!("Job {} is {}", j, &job.state);
                }
                _ => (),
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
    match Args::parse() {
        Args::List {
            verbosity,
            json,
            jobids_or_obsids,
            states,
            types: job_types,
        } => {
            init_logger(verbosity);

            let (jobids, obsids) = parse_many_jobids_or_obsids(&jobids_or_obsids)?;
            let client = AsvoClient::new()?;
            let mut jobs = client.get_jobs()?;
            match (jobids, obsids) {
                (jobids, obsids) if !jobids.is_empty() && !obsids.is_empty() => {
                    bail!("You can't specify both job IDs and obsIDs. Please use one or the other.")
                }
                (jobids, _) if !jobids.is_empty() => {
                    jobs = jobs.retain(|j| jobids.contains(&j.jobid))
                }
                (_, obsids) if !obsids.is_empty() => {
                    jobs = jobs.retain(|j| obsids.contains(&j.obsid))
                }
                _ => (),
            };

            if !job_types.is_empty() {
                jobs = jobs.retain(|j| job_types.contains(&j.jtype))
            }

            if !states.is_empty() {
                jobs = jobs.retain(|j| {
                    states.iter().any(|s|
                        // this allows comparison with AsvoJobState::Error(..)
                        std::mem::discriminant(s) == std::mem::discriminant(&j.state))
                });
            }

            if json {
                println!("{}", jobs.json()?);
            } else {
                jobs.list();
            }
        }

        Args::Download {
            keep_zip,
            skip_hash,
            dry_run,
            verbosity,
            jobids_or_obsids,
            download_dir,
            ..
        } => {
            if jobids_or_obsids.is_empty() {
                bail!("No jobs specified!");
            }
            init_logger(verbosity);

            let (jobids, obsids) = parse_many_jobids_or_obsids(&jobids_or_obsids)?;
            let hash = !skip_hash;
            if dry_run {
                if !jobids.is_empty() {
                    debug!("Parsed job IDs: {:#?}", jobids);
                }
                if !obsids.is_empty() {
                    debug!("Parsed obsids: {:#?}", obsids);
                }
                info!(
                    "Parsed {} jobids and {} obsids for download. keep_zip={:?}, hash={:?}",
                    jobids.len(),
                    obsids.len(),
                    keep_zip,
                    hash,
                );
            } else {
                let client = AsvoClient::new()?;
                for j in jobids {
                    client.download_job(j, keep_zip, hash, &download_dir)?;
                }
                for o in obsids {
                    client.download_obsid(o, keep_zip, hash, &download_dir)?;
                }
            }
        }

        Args::SubmitVis {
            delivery,
            delivery_format,
            wait,
            dry_run,
            allow_resubmit,
            verbosity,
            obsids,
        } => {
            init_logger(verbosity);

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

            let delivery = Delivery::validate(delivery)?;
            debug!("Using {} for delivery", delivery);

            let delivery_format: Option<DeliveryFormat> =
                DeliveryFormat::validate(delivery_format)?;
            debug!("Using {:#?} for delivery format", delivery_format);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for visibility download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                let mut submitted_count = 0;

                for o in parsed_obsids {
                    let j = client.submit_vis(o, delivery, delivery_format, allow_resubmit)?;

                    if j.is_some() {
                        let jobid = j.unwrap();
                        info!("Submitted {} as ASVO job ID {}", o, jobid);
                        jobids.push(jobid);
                        submitted_count += 1;
                    }
                    // for the none case- the "submit_asvo" function
                    // will have already provided user some feedback
                }
                info!(
                    "Submitted {} obsids for visibility download.",
                    submitted_count
                );

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitConv {
            parameters,
            delivery,
            delivery_format,
            wait,
            dry_run,
            allow_resubmit,
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

            let delivery = Delivery::validate(delivery)?;
            debug!("Using {} for delivery", delivery);

            let delivery_format: Option<DeliveryFormat> =
                DeliveryFormat::validate(delivery_format)?;
            debug!("Using {:#?} for delivery format", delivery_format);

            // Get the user parameters and set any defaults that the user has not set.
            let params = {
                let mut params = match &parameters {
                    Some(s) => parse_key_value_pairs(s)?,
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
                let mut submitted_count = 0;

                for o in parsed_obsids {
                    let j = client.submit_conv(
                        o,
                        delivery,
                        delivery_format,
                        &params,
                        allow_resubmit,
                    )?;

                    if j.is_some() {
                        let jobid = j.unwrap();
                        info!("Submitted {} as ASVO job ID {}", o, jobid);
                        jobids.push(jobid);
                        submitted_count += 1;
                    }
                    // for the none case- the "submit_asvo" function
                    // will have already provided user some feedback
                }
                info!("Submitted {} obsids for conversion.", submitted_count);

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitMeta {
            delivery,
            delivery_format,
            wait,
            dry_run,
            allow_resubmit,
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

            let delivery = Delivery::validate(delivery)?;
            debug!("Using {} for delivery", delivery);

            let delivery_format: Option<DeliveryFormat> =
                DeliveryFormat::validate(delivery_format)?;
            debug!("Using {:#?} for delivery format", delivery_format);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for metadata download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());

                let mut submitted_count = 0;
                for o in parsed_obsids {
                    let j = client.submit_meta(o, delivery, delivery_format, allow_resubmit)?;
                    if j.is_some() {
                        let jobid = j.unwrap();
                        info!("Submitted {} as ASVO job ID {}", o, jobid);
                        jobids.push(jobid);
                        submitted_count += 1;
                    }
                    // for the none case- the "submit_asvo" function
                    // will have already provided user some feedback
                }
                info!(
                    "Submitted {} obsids for metadata download.",
                    submitted_count
                );

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitVolt {
            delivery,
            offset,
            duration,
            from_channel,
            to_channel,
            wait,
            dry_run,
            allow_resubmit,
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

            let delivery = Delivery::validate(delivery)?;
            debug!("Using {} for delivery", delivery);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for voltage download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                let mut submitted_count = 0;

                for o in parsed_obsids {
                    let j = client.submit_volt(
                        o,
                        delivery,
                        offset,
                        duration,
                        from_channel,
                        to_channel,
                        allow_resubmit,
                    )?;

                    if j.is_some() {
                        let jobid = j.unwrap();
                        info!("Submitted {} as ASVO job ID {}", o, jobid);
                        jobids.push(jobid);
                        submitted_count += 1;
                    }
                    // for the none case- the "submit_asvo" function
                    // will have already provided user some feedback
                }
                info!("Submitted {} obsids for voltage download.", submitted_count);

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready.
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::Wait {
            verbosity,
            jobs,
            json,
        } => {
            let (parsed_jobids, _) = parse_many_jobids_or_obsids(&jobs)?;
            if parsed_jobids.is_empty() {
                bail!("No obsids specified!");
            }
            init_logger(verbosity);
            let client = AsvoClient::new()?;
            // Endlessly loop over the newly-supplied job IDs until
            // they're all ready.
            wait_loop(&client, &parsed_jobids)?;

            let mut jobs = client.get_jobs()?;
            if !parsed_jobids.is_empty() {
                jobs = jobs.retain(|j| parsed_jobids.contains(&j.jobid));
            }

            if json {
                println!("{}", jobs.json()?);
            } else {
                jobs.list();
            }
        }
    }

    Ok(())
}
