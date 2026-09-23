// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use std::{thread, time};

use anyhow::bail;
use clap::Parser;
use log::{debug, error, info, warn};
use simplelog::*;

use rayon::prelude::*;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;

use mwa_giant_squid::asvo::*;
use mwa_giant_squid::cli::Args;
use mwa_giant_squid::*;

fn create_progress_bar(multi_progress_bar: &MultiProgress) -> ProgressBar {
    let pb = multi_progress_bar.add(ProgressBar::new(0));

    let sty = ProgressStyle::with_template(
        "{spinner:.green} {msg} [{bar:60.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {elapsed_precise}, eta: {eta})",
    )
    .expect("Unable to create progress bar style");
    pb.set_style(sty);

    pb
}

fn run_jobid_download(jobid: AsvoJobID, opts: &DownloadOptions) -> anyhow::Result<()> {
    // Add a small delay to hopefully have the downloads start in order
    // (this is just a log display thing! So 1/2 shows before 2/2 (at least initially!))
    thread::sleep(time::Duration::from_millis(100));

    let client = AsvoClient::new()?;
    client.download_jobid(jobid, opts)?;
    Ok(())
}

fn run_obsid_download(obsid: Obsid, opts: &DownloadOptions) -> anyhow::Result<()> {
    // Add a small delay to hopefully have the downloads start in order
    // (this is just a log display thing! So 1/2 shows before 2/2 (at least initially!))
    thread::sleep(time::Duration::from_millis(100));

    let client = AsvoClient::new()?;
    client.download_obsid(obsid, opts)?;
    Ok(())
}

fn init_logger(level: u8) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in SimpleLogger")
        .build();
    match level {
        0 => SimpleLogger::init(LevelFilter::Info, log_config).unwrap(),
        1 => SimpleLogger::init(LevelFilter::Debug, log_config).unwrap(),
        _ => SimpleLogger::init(LevelFilter::Trace, log_config).unwrap(),
    };
}

fn init_logger_with_progressbar_support(level: u8, multiprogressbar: &MultiProgress) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in SimpleLogger")
        .build();

    let filter = match level {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    let log = SimpleLogger::new(filter, log_config);

    LogWrapper::new(multiprogressbar.clone(), log)
        .try_init()
        .unwrap();
}

/// Wait for all of the specified job IDs to become ready, then exit.
/// Polls via `AsvoClient::get_jobs`.
fn wait_loop(client: &AsvoClient, jobids: &[AsvoJobID]) -> anyhow::Result<()> {
    info!("Waiting for {} jobs to be ready...", jobids.len());
    let mut last_state = BTreeMap::<AsvoJobID, AsvoJobState>::new();
    // Offer the MWA ASVO a kindness by waiting a few seconds, so
    // that the user's queue is hopefully current.
    std::thread::sleep(Duration::from_secs(1));
    loop {
        // Get the current state of all jobs. By converting to a map, we avoid
        // quadratic complexity below. Probably not a big deal, but why not?
        // `None` here mirrors `list`'s own default: fetch full history
        // rather than relying on the (unconfirmed) server-side default.
        let jobs = client.get_jobs(None)?.into_map();
        let mut any_not_ready = false;
        // Iterate over all supplied job IDs.
        for j in jobids {
            // Find the relevant job in the queue.
            let job = match jobs.0.get(j) {
                None => bail!("MWA ASVO job ID {} wasn't found in your list of jobs.", j),
                Some(job) => job,
            };
            // Handle the job's state. If it's ready, there's nothing to do. If
            // the job is simply queued or in processing (or other intermediate states),
            // we can say that we're not ready yet. All other possibilities are handled drastically.
            match &job.state {
                AsvoJobState::Ready => (),
                AsvoJobState::Error(e) => {
                    bail!(
                        "MWA ASVO job ID {} (obsid: {}) has an error: {}",
                        j,
                        job.obsid,
                        e
                    );
                }
                AsvoJobState::Expired => bail!("MWA ASVO job ID {} has expired.", j),
                AsvoJobState::Cancelled => bail!("MWA ASVO job ID {} has been cancelled.", j),
                _ => {
                    // For all other states
                    any_not_ready = true;
                }
            }
            // log if there was a change in state.
            let log_prefix = format!("Job ID {} (obsid: {}):", job.jobid, job.obsid);
            match last_state.insert(*j, job.state.clone()) {
                Some(last_state) if last_state != job.state => {
                    info!("{} is {}", log_prefix, job.state);
                }
                Some(_) => (), // State did not change from last_state
                None => info!("{} is {}", log_prefix, job.state), // First time just report current state
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
    info!("All {} MWA ASVO jobs are ready for download.", jobids.len());
    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    match Args::parse() {
        Args::List {
            verbosity,
            json,
            jobids_or_obsids,
            states,
            no_colour,
            days,
            types: job_types,
        } => {
            init_logger(verbosity);

            let (jobids, obsids) = parse_many_jobids_or_obsids(&jobids_or_obsids)?;
            let client = AsvoClient::new()?;
            let mut jobs = client.get_jobs(days)?;
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
                jobs.list(no_colour);
            }
        }

        Args::Download {
            keep_tar: keep_zip,
            no_resume,
            concurrent_downloads,
            skip_hash,
            dry_run,
            verbosity,
            jobids_or_obsids,
            download_dir,
            ..
        } => {
            if jobids_or_obsids.is_empty() {
                bail!("No jobs or obsids specified!");
            }

            // Validate the download directory
            if !Path::new(&download_dir).exists() {
                bail!(
                    "Download directory `{}` does not exist or is not accessible.",
                    download_dir
                );
            }

            // Create progress bar capable of multiple downloads
            let mpb = MultiProgress::new();

            // Init the logger- special case as we need to use LogWrapper to ensure log
            // messages don't mess up the progress bars!
            init_logger_with_progressbar_support(verbosity, &mpb);

            rayon::ThreadPoolBuilder::new()
                .num_threads(concurrent_downloads)
                .build_global()
                .unwrap();

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
                // Each download will report an error if there is one, so no need to do anything with
                // the results (I think)
                let t: usize = jobids.len() + obsids.len();

                let mut jobids_results: Vec<anyhow::Result<()>> = jobids
                    .par_iter()
                    .enumerate()
                    .map(|(c, j)| {
                        let pb = create_progress_bar(&mpb);
                        let opts = DownloadOptions {
                            keep_tar: keep_zip,
                            no_resume,
                            hash,
                            download_dir: &download_dir,
                            progress_bar: &pb,
                            download_number: c + 1,
                            download_count: t,
                        };
                        run_jobid_download(*j, &opts)
                    })
                    .collect();

                let mut obsids_results: Vec<anyhow::Result<()>> = obsids
                    .par_iter()
                    .enumerate()
                    .map(|(c, o)| {
                        let pb = create_progress_bar(&mpb);
                        let opts = DownloadOptions {
                            keep_tar: keep_zip,
                            no_resume,
                            hash,
                            download_dir: &download_dir,
                            progress_bar: &pb,
                            download_number: c + 1,
                            download_count: t,
                        };
                        run_obsid_download(*o, &opts)
                    })
                    .collect();

                // Combine both sets of results
                // Filter for only Errors
                // Report each error
                for job_result in jobids_results
                    .iter_mut()
                    .chain(obsids_results.iter_mut())
                    .filter(|o| o.is_err())
                {
                    error!("{}", job_result.as_mut().unwrap_err());
                }
            }
        }

        Args::SubmitVis {
            download,
            wait,
            dry_run,
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
                    let obs_id_i64 = i64::try_from(u64::from(o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = download.to_vis_params(obs_id_i64)?;

                    let resp = client.submit_download_vis_job(&params)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(u64::from(job_id)) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }
                info!(
                    "Submitted {} obsids for visibility download.",
                    submitted_count
                );

                if wait {
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitConv {
            conv,
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
                    "Would have submitted {} obsids for conversion with: \
                     output={:?}, avg_freq_res={}, avg_time_res={}, \
                     flag_edge_width={}, centre={:?}",
                    obsids.len(),
                    conv.output,
                    conv.avg_freq_res,
                    conv.avg_time_res,
                    conv.flag_edge_width,
                    conv.centre
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                let mut submitted_count = 0;

                for o in parsed_obsids {
                    let obs_id_i64 = i64::try_from(u64::from(o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = conv.to_params(obs_id_i64)?;

                    let resp = client.submit_conversion_job(&params)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(u64::from(job_id)) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }
                info!("Submitted {} obsids for conversion.", submitted_count);

                if wait {
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitImage {
            image,
            wait,
            dry_run,
            verbosity,
            obsids,
        } => {
            if obsids.is_empty() {
                bail!("No obsids specified!");
            }

            let (jobids_from_input, obsids) = parse_many_jobids_or_obsids(&obsids)?;
            if !jobids_from_input.is_empty() {
                bail!(
                    "This command only accepts obsids; to image an existing conversion job, use submit-image-from-job instead."
                );
            }

            init_logger(verbosity);

            if dry_run {
                info!(
                    "Would have submitted {} obsids for imaging with: delivery={:?}, delivery_format={:?}, image_size={:?}, weighting={:?}, output_mode={:?}, phase_center={:?}",
                    obsids.len(),
                    image.delivery,
                    image.delivery_format,
                    image.image_size,
                    image.weighting,
                    image.output_mode,
                    image.phase_center
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                let mut submitted_count = 0;

                for o in &obsids {
                    let obs_id_i64 = i64::try_from(u64::from(*o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = image.to_params(obs_id_i64)?;

                    let job_id = client.submit_imaging_job(&params)?;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(job_id) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }

                info!("Submitted {} obsids for imaging.", submitted_count);

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready. Reuses the v2 client's own
                    // get_jobs, so this polls the same v2 API we just
                    // submitted to.
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitImageFromJob {
            image,
            wait,
            dry_run,
            verbosity,
            obsids,
        } => {
            if obsids.is_empty() {
                bail!("No obsids specified!");
            }

            let (jobids_from_input, obsids) = parse_many_jobids_or_obsids(&obsids)?;
            if !jobids_from_input.is_empty() {
                bail!("This command only accepts obsids, not job IDs.");
            }

            if obsids.len() != 1 {
                bail!(
                    "submit-image-from-job requires exactly one obsid \
                     (the source_job_id identifies the conversion job for that obsid)."
                );
            }

            init_logger(verbosity);

            if dry_run {
                info!(
                    "Would have submitted obsid {} for image-from-job with: \
                     source_job_id={}, delivery={:?}, delivery_format={:?}, \
                     image_size={:?}, weighting={:?}, output_mode={:?}",
                    obsids[0],
                    image.source_job_id,
                    image.delivery,
                    image.delivery_format,
                    image.image_size,
                    image.weighting,
                    image.output_mode
                );
            } else {
                let client = AsvoClient::new()?;

                let o = &obsids[0];
                let obs_id_i64 = i64::try_from(u64::from(*o))
                    .expect("Obsid's validated range always fits in i64");

                let params = image.to_params(obs_id_i64)?;

                let job_id = client.submit_image_from_job(&params)?;
                info!("Submitted {} as MWA ASVO image-from-job ID {}", o, job_id);

                if wait {
                    match AsvoJobID::try_from(job_id) {
                        Ok(id) => wait_loop(&client, &[id])?,
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; cannot --wait",
                            job_id
                        ),
                    }
                }
            }
        }

        Args::SubmitMeta {
            download,
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

                let mut submitted_count = 0;
                for o in parsed_obsids {
                    let obs_id_i64 = i64::try_from(u64::from(o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = download.to_meta_params(obs_id_i64)?;

                    let resp = client.submit_download_vis_job(&params)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(u64::from(job_id)) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }
                info!(
                    "Submitted {} obsids for metadata download.",
                    submitted_count
                );

                if wait {
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitVolt {
            volt,
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
                    "Would have submitted {} obsids for voltage download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());
                let mut submitted_count = 0;

                for o in parsed_obsids {
                    let obs_id_i64 = i64::try_from(u64::from(o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = volt.to_params(obs_id_i64)?;

                    let resp = client.submit_voltage_job(&params)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(u64::from(job_id)) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }
                info!("Submitted {} obsids for voltage download.", submitted_count);

                if wait {
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::SubmitBf {
            bf,
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
                    "Would have submitted {} obsids for beamformer download.",
                    obsids.len()
                );
            } else {
                let client = AsvoClient::new()?;
                let mut jobids: Vec<AsvoJobID> = Vec::with_capacity(obsids.len());

                let mut submitted_count = 0;
                for o in parsed_obsids {
                    let obs_id_i64 = i64::try_from(u64::from(o))
                        .expect("Obsid's validated range always fits in i64");

                    let params = bf.to_params(obs_id_i64)?;

                    let resp = client.submit_beamformer_job(&params)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    match AsvoJobID::try_from(u64::from(job_id)) {
                        Ok(id) => jobids.push(id),
                        Err(_) => warn!(
                            "MWA ASVO job ID {} doesn't fit in the expected range; --wait won't track it",
                            job_id
                        ),
                    }
                    submitted_count += 1;
                }
                info!(
                    "Submitted {} obsids for beamformer download.",
                    submitted_count
                );

                if wait {
                    wait_loop(&client, &jobids)?;
                }
            }
        }

        Args::Wait {
            verbosity,
            jobs,
            json,
            no_colour,
        } => {
            let (parsed_jobids, _) = parse_many_jobids_or_obsids(&jobs)?;
            if parsed_jobids.is_empty() {
                bail!("No jobids specified!");
            }
            init_logger(verbosity);
            let client = AsvoClient::new()?;
            // Endlessly loop over the newly-supplied job IDs until
            // they're all ready.
            wait_loop(&client, &parsed_jobids)?;

            let mut jobs = client.get_jobs(None)?;
            if !parsed_jobids.is_empty() {
                jobs = jobs.retain(|j| parsed_jobids.contains(&j.jobid));
            }

            if json {
                println!("{}", jobs.json()?);
            } else {
                jobs.list(no_colour);
            }
        }

        Args::Cancel {
            dry_run,
            verbosity,
            jobs,
        } => {
            let (parsed_jobids, _) = parse_many_jobids_or_obsids(&jobs)?;
            if parsed_jobids.is_empty() {
                bail!("No jobids specified!");
            }
            init_logger(verbosity);

            if dry_run {
                info!("Would have cancelled {} jobids.", parsed_jobids.len());
            } else {
                let client = AsvoClient::new()?;

                let mut cancelled_count = 0;
                for j in parsed_jobids {
                    match client.cancel_job(j) {
                        Ok(resp) => {
                            info!("Cancelled MWA ASVO job ID {} ({})", j, resp.message);
                            cancelled_count += 1;
                        }
                        Err(e) => {
                            error!("Failed to cancel MWA ASVO job ID {}: {}", j, e);
                        }
                    }
                }
                info!("Cancelled {} jobs.", cancelled_count);
            }
        }
    }

    Ok(())
}
