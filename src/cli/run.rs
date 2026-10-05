// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `giant-squid` command: [`run_cli`] runs it.
//!
//! It is in the library, not in `src/bin`, so that the Python module can run
//! the same code as the `giant-squid` program (see `docs/PYTHON_BINDINGS.md`).
//! The command parses its arguments with [`super::Args`] and does what they
//! say with the library; the program and the Python launcher only call
//! [`run_cli`].

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

use crate::asvo::apiv2::openapi::JobSubmittedResponse;
use crate::asvo::*;
use crate::*;

use super::legacy_json::{to_legacy_json, LEGACY_JSON_WARNING};
use super::table::print_jobs_table;
use super::Args;

fn create_progress_bar(multi_progress_bar: &MultiProgress) -> ProgressBar {
    let pb = multi_progress_bar.add(ProgressBar::new(0));

    let sty = ProgressStyle::with_template(
        "{spinner:.green} {msg} [{bar:60.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {elapsed_precise}, eta: {eta})",
    )
    .expect("Unable to create progress bar style");
    pb.set_style(sty);

    pb
}

/// How often a progress bar's spinner redraws while a download runs.
const PROGRESS_BAR_TICK: Duration = Duration::from_millis(500);

/// Show a library [`DownloadProgress`] event on an `indicatif` bar.
fn update_progress_bar(pb: &ProgressBar, event: DownloadProgress) {
    match event {
        DownloadProgress::Started {
            label,
            total_bytes,
            position,
            ..
        } => {
            pb.enable_steady_tick(PROGRESS_BAR_TICK);
            pb.set_length(total_bytes);
            pb.set_position(position);
            pb.reset_eta();
            pb.set_message(label);
        }
        DownloadProgress::Advanced { bytes } => pb.inc(bytes),
        DownloadProgress::Finished => pb.finish_and_clear(),
    }
}

/// Log in to the MWA ASVO with the config from the environment.
fn connect() -> anyhow::Result<AsvoClient> {
    Ok(AsvoClient::new(client_config_from_env()?)?)
}

fn run_job_id_download(job_id: AsvoJobId, opts: &DownloadOptions) -> anyhow::Result<()> {
    // Add a small delay to hopefully have the downloads start in order
    // (this is just a log display thing! So 1/2 shows before 2/2 (at least initially!))
    thread::sleep(time::Duration::from_millis(100));

    let client = connect()?;
    client.download_job(job_id, opts)?;
    Ok(())
}

fn run_obs_id_download(obs_id: ObsId, opts: &DownloadOptions) -> anyhow::Result<()> {
    // Add a small delay to hopefully have the downloads start in order
    // (this is just a log display thing! So 1/2 shows before 2/2 (at least initially!))
    thread::sleep(time::Duration::from_millis(100));

    let client = connect()?;
    client.download_obs(obs_id, opts)?;
    Ok(())
}

/// Submit one job per obsid, carrying on past failures.
///
/// A bad obsid in a list used to abort the whole run on the first error,
/// hiding whatever came after it. Each failure is reported as it happens,
/// the successes still go through, and the run ends with a summary. An
/// error is returned when anything failed, so the exit code still signals
/// it, but only after every obsid has been attempted. The error only counts
/// the failures, because each one was already reported.
fn submit_each_obs_id<F>(
    obs_ids: &[ObsId],
    description: &str,
    mut submit: F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(&ObsId, i64) -> Result<(), anyhow::Error>,
{
    let mut failures: usize = 0;

    for o in obs_ids {
        let obs_id_i64 =
            i64::try_from(u64::from(*o)).expect("Obsid's validated range always fits in i64");

        if let Err(e) = submit(o, obs_id_i64) {
            error!("Obsid {}: {}", o, e);
            failures += 1;
        }
    }

    let submitted = obs_ids.len() - failures;
    info!(
        "Submitted {} of {} obsids for {}.",
        submitted,
        obs_ids.len(),
        description
    );

    if failures == 0 {
        return Ok(());
    }

    bail!("{} of {} job submissions failed", failures, obs_ids.len());
}

/// The message of `submit-image` for a job ID: this command takes obsids
/// only, and `submit-image-from-job` is the command for a conversion job.
const IMAGE_JOB_IDS_MESSAGE: &str = "This command only accepts obsids; to image an existing conversion job, use submit-image-from-job instead.";

/// The message of `submit-image-from-job` for a job ID among its arguments.
/// The command takes a conversion job, but as `--source-job-id`.
const IMAGE_FROM_JOB_JOB_IDS_MESSAGE: &str =
    "The arguments must be obsids, not job IDs. Give the conversion job with --source-job-id.";

/// Parse the obsids of a submit command with the library's
/// [`parse_obs_ids_only`]. `job_ids_message` is the command's own text for
/// the error of a job ID, or `None` for the library's text.
fn obs_ids_only(
    strings: &[String],
    job_ids_message: Option<&str>,
) -> Result<Vec<ObsId>, anyhow::Error> {
    parse_obs_ids_only(strings).map_err(|e| match (e, job_ids_message) {
        (ParseError::JobIdsGiven { .. }, Some(message)) => anyhow::anyhow!("{message}"),
        (e, _) => e.into(),
    })
}

/// Print a submission's response as one line of JSON, for `--json`.
///
/// One compact object per submitted job, in submission order, so a caller
/// can read the job IDs back without parsing log text.
fn print_submitted_json(resp: &JobSubmittedResponse, json: bool) -> Result<(), anyhow::Error> {
    if json {
        println!("{}", serde_json::to_string(resp)?);
    }
    Ok(())
}

/// Report what a submission would have sent, for `--dry-run`.
///
/// Every submit command prints the same thing: the endpoint the request
/// would go to, and the resolved JSON body for each obsid. The body is
/// built exactly as a real submission builds it, so a dry run exercises
/// the argument-to-request mapping rather than just echoing arguments.
fn report_dry_run_submissions<T, F>(
    endpoint: &str,
    obs_ids: &[ObsId],
    build_params: F,
) -> Result<(), anyhow::Error>
where
    T: serde::Serialize,
    F: Fn(i64) -> Result<T, AsvoApiError>,
{
    for o in obs_ids {
        let obs_id_i64 =
            i64::try_from(u64::from(*o)).expect("Obsid's validated range always fits in i64");
        let params = build_params(obs_id_i64)?;
        info!(
            "[dry run] Would POST {} for obsid {}:\n{}",
            endpoint,
            o,
            serde_json::to_string_pretty(&params)?
        );
    }

    info!(
        "[dry run] Would have submitted {} obsids to {}. Nothing was sent.",
        obs_ids.len(),
        endpoint
    );
    Ok(())
}

/// The log level for a `-v` count: none is `Info`, one is `Debug`, more is
/// `Trace`.
fn log_level(verbosity: u8) -> LevelFilter {
    match verbosity {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
}

/// Send all log records to stderr, so that stdout has only a command's
/// output (the job table, `--json`), which a script can read. The
/// `simplelog` `SimpleLogger` sent every level except `Error` to stdout.
///
/// There is one logger per process. If one is already installed (the
/// command ran before in this process, or another program set one), it is
/// kept and only the level changes: the command has no other use for the
/// error.
fn init_logger(verbosity: u8) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in the logger")
        .build();
    let level = log_level(verbosity);
    if WriteLogger::init(level, log_config, std::io::stderr()).is_err() {
        log::set_max_level(level);
    }
}

fn init_logger_with_progressbar_support(level: u8, multiprogressbar: &MultiProgress) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in the logger")
        .build();

    // To stderr, as in `init_logger`.
    let log = WriteLogger::new(log_level(level), log_config, std::io::stderr());

    // As in `init_logger`, a logger that is already installed is kept.
    if LogWrapper::new(multiprogressbar.clone(), log)
        .try_init()
        .is_err()
    {
        log::set_max_level(log_level(level));
    }
}

/// Poll the job list until all of `job_ids` are ready, logging each job's
/// state when it changes. Fails as soon as a job is missing, has an error,
/// or has been cancelled (see `AsvoJobVec::all_ready`).
fn wait_loop(client: &AsvoClient, job_ids: &[AsvoJobId]) -> anyhow::Result<()> {
    info!("Waiting for {} jobs to be ready...", job_ids.len());
    let mut last_state = BTreeMap::<AsvoJobId, JobState>::new();
    // Offer the MWA ASVO a kindness by waiting a moment, so that the
    // user's queue is hopefully current.
    std::thread::sleep(WAIT_INITIAL_DELAY);
    loop {
        // The default window of the API, as `list` without `--days` has.
        let jobs = client.get_jobs(&JobsFilter::default())?;
        let all_ready = jobs.all_ready(job_ids)?;

        // Log if there was a change in state. `all_ready` has already
        // checked that every job is in the list.
        for job in job_ids
            .iter()
            .filter_map(|id| jobs.0.iter().find(|j| j.job_id == *id))
        {
            let log_prefix = format!("Job ID {} (obsid: {}):", job.job_id, job.obs_id);
            match last_state.insert(job.job_id, job.job_state) {
                Some(last_state) if last_state != job.job_state => {
                    info!("{} is {}", log_prefix, job.job_state);
                }
                Some(_) => (), // State did not change from last_state
                None => info!("{} is {}", log_prefix, job.job_state), // First time just report current state
            }
        }

        if all_ready {
            break;
        }
        std::thread::sleep(WAIT_POLL_INTERVAL);
    }
    info!(
        "All {} MWA ASVO jobs are ready for download.",
        job_ids.len()
    );
    Ok(())
}

/// Run a command that clap has parsed.
fn run(args: Args) -> anyhow::Result<()> {
    match args {
        Args::List {
            verbosity,
            json,
            legacy_json,
            job_ids_or_obs_ids,
            job_states,
            no_colour,
            days,
            job_types,
            date_from,
            date_to,
            sort_by,
        } => {
            init_logger(verbosity);

            let (job_ids, obs_ids) = parse_many_job_ids_or_obs_ids(&job_ids_or_obs_ids)?;
            let query = JobQuery {
                job_ids,
                obs_ids,
                job_types,
                job_states,
                days,
                date_from,
                date_to,
                sort_by,
            };
            // Before connecting, so that a bad query does not log in.
            query.validate()?;
            let client = connect()?;
            let jobs = client.list_jobs(&query)?;

            if legacy_json {
                warn!("{LEGACY_JSON_WARNING}");
                println!("{}", to_legacy_json(&jobs)?);
            } else if json {
                println!("{}", jobs.json()?);
            } else {
                print_jobs_table(jobs, no_colour);
            }
        }

        Args::Download {
            keep_tar,
            no_resume,
            concurrent_downloads,
            skip_hash,
            dry_run,
            verbosity,
            job_ids_or_obs_ids,
            download_dir,
            ..
        } => {
            if job_ids_or_obs_ids.is_empty() {
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

            let (job_ids, obs_ids) = parse_many_job_ids_or_obs_ids(&job_ids_or_obs_ids)?;
            let hash = !skip_hash;
            let DownloadSettings {
                buffer_size,
                retry_duration,
            } = DownloadSettings::from_env()?;
            if dry_run {
                if !job_ids.is_empty() {
                    debug!("Parsed job IDs: {:#?}", job_ids);
                }
                if !obs_ids.is_empty() {
                    debug!("Parsed obsids: {:#?}", obs_ids);
                }
                info!(
                    "Parsed {} jobids and {} obsids for download. keep_tar={:?}, hash={:?}",
                    job_ids.len(),
                    obs_ids.len(),
                    keep_tar,
                    hash,
                );
            } else {
                // Each download will report an error if there is one, so no need to do anything with
                // the results (I think)
                let t: usize = job_ids.len() + obs_ids.len();

                let mut job_ids_results: Vec<anyhow::Result<()>> = job_ids
                    .par_iter()
                    .enumerate()
                    .map(|(c, j)| {
                        let pb = create_progress_bar(&mpb);
                        let progress = |event| update_progress_bar(&pb, event);
                        let opts = DownloadOptions {
                            keep_tar,
                            no_resume,
                            hash,
                            download_dir: &download_dir,
                            progress: Some(&progress),
                            download_number: c + 1,
                            download_count: t,
                            buffer_size,
                            retry_duration,
                            // Ctrl-C ends the CLI process, as before.
                            should_stop: None,
                        };
                        run_job_id_download(*j, &opts)
                    })
                    .collect();

                let mut obs_ids_results: Vec<anyhow::Result<()>> = obs_ids
                    .par_iter()
                    .enumerate()
                    .map(|(c, o)| {
                        let pb = create_progress_bar(&mpb);
                        let progress = |event| update_progress_bar(&pb, event);
                        let opts = DownloadOptions {
                            keep_tar,
                            no_resume,
                            hash,
                            download_dir: &download_dir,
                            progress: Some(&progress),
                            download_number: c + 1,
                            download_count: t,
                            buffer_size,
                            retry_duration,
                            // Ctrl-C ends the CLI process, as before.
                            should_stop: None,
                        };
                        run_obs_id_download(*o, &opts)
                    })
                    .collect();

                // Every download runs to completion before anything is
                // reported, so one failure doesn't hide the rest. Report
                // each error, then fail the run as a whole so a script can
                // tell something went wrong.
                let mut failures = 0;
                for job_result in job_ids_results
                    .iter_mut()
                    .chain(obs_ids_results.iter_mut())
                    .filter(|o| o.is_err())
                {
                    error!("{}", job_result.as_mut().unwrap_err());
                    failures += 1;
                }

                info!("Downloaded {} of {}.", t - failures, t);

                if failures > 0 {
                    bail!(
                        "{} of {} downloads failed; see the errors above.",
                        failures,
                        t
                    );
                }
            }
        }

        Args::SubmitVis {
            download,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            init_logger(verbosity);

            let parsed_obs_ids = obs_ids_only(&obs_ids, None)?;

            if dry_run {
                report_dry_run_submissions(ENDPOINT_DOWNLOAD_VIS_JOB, &parsed_obs_ids, |obs_id| {
                    download.to_vis_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome =
                    submit_each_obs_id(&parsed_obs_ids, "visibility download", |o, id| {
                        let params = download.to_vis_params(id)?;
                        let resp = client.submit_download_vis_job(&params)?;
                        print_submitted_json(&resp, json)?;
                        let job_id = resp.job_id;
                        info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                        job_ids.push(job_id.get());
                        Ok(())
                    });

                if wait {
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::SubmitConv {
            conv,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let parsed_obs_ids = obs_ids_only(&obs_ids, None)?;
            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_CONVERSION_JOB, &parsed_obs_ids, |obs_id| {
                    conv.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome = submit_each_obs_id(&parsed_obs_ids, "conversion", |o, id| {
                    let params = conv.to_params(id)?;
                    let resp = client.submit_conversion_job(&params)?;
                    print_submitted_json(&resp, json)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    job_ids.push(job_id.get());
                    Ok(())
                });

                if wait {
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::SubmitImage {
            image,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_JOB_IDS_MESSAGE))?;

            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_IMAGING_JOB, &obs_ids, |obs_id| {
                    image.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome = submit_each_obs_id(&obs_ids, "imaging", |o, id| {
                    let params = image.to_params(id)?;
                    let resp = client.submit_imaging_job(&params)?;
                    print_submitted_json(&resp, json)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    job_ids.push(job_id.get());
                    Ok(())
                });

                if wait {
                    // Endlessly loop over the newly-supplied job IDs until
                    // they're all ready. Reuses the v2 client's own
                    // get_jobs, so this polls the same v2 API we just
                    // submitted to.
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::SubmitImageFromJob {
            image,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_FROM_JOB_JOB_IDS_MESSAGE))?;

            if obs_ids.len() != 1 {
                bail!(
                    "submit-image-from-job requires exactly one obsid \
                     (the source_job_id identifies the conversion job for that obsid)."
                );
            }

            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_IMAGE_FROM_JOB, &obs_ids, |obs_id| {
                    image.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;

                let o = &obs_ids[0];
                let obs_id_i64 = i64::try_from(u64::from(*o))
                    .expect("Obsid's validated range always fits in i64");

                let params = image.to_params(obs_id_i64)?;

                let resp = client.submit_image_from_job(&params)?;
                print_submitted_json(&resp, json)?;
                let job_id = resp.job_id;
                info!("Submitted {} as MWA ASVO image-from-job ID {}", o, job_id);

                if wait {
                    wait_loop(&client, &[job_id.get()])?;
                }
            }
        }

        Args::SubmitMeta {
            download,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let parsed_obs_ids = obs_ids_only(&obs_ids, None)?;
            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_DOWNLOAD_VIS_JOB, &parsed_obs_ids, |obs_id| {
                    download.to_meta_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome = submit_each_obs_id(&parsed_obs_ids, "metadata download", |o, id| {
                    let params = download.to_meta_params(id)?;
                    let resp = client.submit_download_meta_job(&params)?;
                    print_submitted_json(&resp, json)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    job_ids.push(job_id.get());
                    Ok(())
                });

                if wait {
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::SubmitVolt {
            volt,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let parsed_obs_ids = obs_ids_only(&obs_ids, None)?;
            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_VOLTAGE_JOB, &parsed_obs_ids, |obs_id| {
                    volt.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome = submit_each_obs_id(&parsed_obs_ids, "voltage download", |o, id| {
                    let params = volt.to_params(id)?;
                    let resp = client.submit_voltage_job(&params)?;
                    print_submitted_json(&resp, json)?;
                    let job_id = resp.job_id;
                    info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                    job_ids.push(job_id.get());
                    Ok(())
                });

                if wait {
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::SubmitBf {
            bf,
            wait,
            dry_run,
            json,
            verbosity,
            obs_ids,
        } => {
            let parsed_obs_ids = obs_ids_only(&obs_ids, None)?;
            init_logger(verbosity);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_BEAMFORMER_JOB, &parsed_obs_ids, |obs_id| {
                    bf.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;
                let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());

                let outcome =
                    submit_each_obs_id(&parsed_obs_ids, "beamformer download", |o, id| {
                        let params = bf.to_params(id)?;
                        let resp = client.submit_beamformer_job(&params)?;
                        print_submitted_json(&resp, json)?;
                        let job_id = resp.job_id;
                        info!("Submitted {} as MWA ASVO job ID {}", o, job_id);
                        job_ids.push(job_id.get());
                        Ok(())
                    });

                if wait {
                    wait_loop(&client, &job_ids)?;
                }

                outcome?;
            }
        }

        Args::Wait {
            verbosity,
            jobs,
            json,
            legacy_json,
            no_colour,
        } => {
            let parsed_job_ids = parse_job_ids_only(&jobs)?;
            init_logger(verbosity);
            let client = connect()?;
            // Endlessly loop over the newly-supplied job IDs until
            // they're all ready.
            wait_loop(&client, &parsed_job_ids)?;

            let jobs = client.list_jobs(&JobQuery {
                job_ids: parsed_job_ids,
                ..JobQuery::default()
            })?;

            if legacy_json {
                warn!("{LEGACY_JSON_WARNING}");
                println!("{}", to_legacy_json(&jobs)?);
            } else if json {
                println!("{}", jobs.json()?);
            } else {
                print_jobs_table(jobs, no_colour);
            }
        }

        Args::Cancel {
            dry_run,
            verbosity,
            jobs,
        } => {
            let parsed_job_ids = parse_job_ids_only(&jobs)?;
            init_logger(verbosity);

            if dry_run {
                for j in &parsed_job_ids {
                    info!("[dry run] Would DELETE {}/{}", ENDPOINT_JOBS, j);
                }
                info!(
                    "[dry run] Would have cancelled {} jobids. Nothing was sent.",
                    parsed_job_ids.len()
                );
            } else {
                let client = connect()?;

                // A reply is not proof that a job was cancelled: the server
                // answers a job that is already cancelled with a normal
                // reply whose message says so. So the logs report requests.
                let sent_count = parsed_job_ids.len();
                let mut failed_count = 0;
                for j in parsed_job_ids {
                    match client.cancel_job(j) {
                        Ok(resp) => {
                            info!("Cancel request for job {}: {}", j, resp.message);
                        }
                        Err(e) => {
                            error!("Failed to cancel MWA ASVO job ID {}: {}", j, e);
                            failed_count += 1;
                        }
                    }
                }
                info!(
                    "Cancel requests: {} sent, {} failed.",
                    sent_count, failed_count
                );
            }
        }
    }

    Ok(())
}

/// Run the `giant-squid` command with the arguments `args`, the first of
/// which is the name of the program, and return the exit code.
///
/// This is the whole of the command. The `giant-squid` program calls it with
/// its own arguments, and the Python module calls it for the Python
/// `giant-squid` command (`mwa_giant_squid._run_cli`), so both run the same
/// code.
///
/// What it does is what `main` of a clap program does: the help and the
/// version go to standard output with code 0; a bad argument goes to
/// standard error with code 2 (clap's own text and codes); an error of a
/// command goes to standard error as `Error: ...`, with code 1. Nothing here
/// ends the process, so the caller can clean up first. The command installs
/// the process's log record logger on its first run, and a later run in the
/// same process uses it.
pub fn run_cli<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let code = match Args::try_parse_from(args) {
        Err(error) => {
            // Printing can fail (a closed pipe); the exit code is the same.
            let _ = error.print();
            error.exit_code()
        }
        Ok(args) => match run(args) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Error: {error:?}");
                EXIT_CODE_FAILED
            }
        },
    };
    // The process may end without Rust's own clean-up (it does when Python
    // ends it), so push out what is still held back.
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    code
}

/// The exit code of a command that failed. (clap's code for a bad argument
/// is 2.)
pub const EXIT_CODE_FAILED: i32 = 1;
