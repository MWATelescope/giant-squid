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
use std::thread;
use std::time::Duration;

use clap::Parser;
use log::{debug, error, info, warn};
use simplelog::*;

use rayon::prelude::*;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use indicatif_log_bridge::LogWrapper;

use crate::mwa_asvo::api::openapi::{JobSubmittedResponse, Status, Type as FileType};
use crate::mwa_asvo::*;
use crate::*;

use super::json_output::{ArgumentError, JsonError, ReportedFailures};
use super::legacy_json::{to_legacy_json, LEGACY_JSON_WARNING};
use super::table::{job_files, job_size_text, print_jobs_table};
use super::{Args, SubmitOptions};

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

/// The wait before each download starts, so that the downloads start (and
/// log) in their order: 1/2 before 2/2. It is for the display only.
const DOWNLOAD_START_DELAY: Duration = Duration::from_millis(100);

/// What one download is for: a Job ID, or an Obs ID.
#[derive(Clone, Copy)]
enum DownloadTarget {
    Job(AsvoJobId),
    Obs(ObsId),
}

impl DownloadTarget {
    /// The targets of `job_ids` and then of `obs_ids`, in that order.
    fn all(job_ids: &[AsvoJobId], obs_ids: &[ObsId]) -> Vec<Self> {
        job_ids
            .iter()
            .map(|j| Self::Job(*j))
            .chain(obs_ids.iter().map(|o| Self::Obs(*o)))
            .collect()
    }

    /// The Job ID that was asked for, if it is one.
    fn job_id(self) -> Option<AsvoJobId> {
        match self {
            Self::Job(job_id) => Some(job_id),
            Self::Obs(_) => None,
        }
    }

    /// The Obs ID that was asked for, if it is one.
    fn obs_id(self) -> Option<ObsId> {
        match self {
            Self::Job(_) => None,
            Self::Obs(obs_id) => Some(obs_id),
        }
    }

    /// Download the target.
    fn download(self, client: &AsvoClient, opts: &DownloadOptions) -> anyhow::Result<AsvoJob> {
        thread::sleep(DOWNLOAD_START_DELAY);
        Ok(match self {
            Self::Job(job_id) => client.download_job(job_id, opts)?,
            Self::Obs(obs_id) => client.download_obs(obs_id, opts)?,
        })
    }
}

/// The result of one successful download, for `download --json`: the keys
/// of the MWA ASVO's replies (`job_id`, `status`, `message`), and the Obs ID.
/// A failed download is a [`JsonError`] line.
#[derive(serde::Serialize)]
struct DownloadReport {
    job_id: AsvoJobId,
    obs_id: ObsId,
    status: Status,
    message: String,
}

impl DownloadReport {
    /// The report of the download of `job`.
    fn new(job: &AsvoJob, download_dir: &str) -> Self {
        Self {
            job_id: job.job_id(),
            obs_id: job.obs_id(),
            status: Status::Success,
            message: downloaded_message(job, download_dir),
        }
    }
}

/// Print the line of one download for `download --json`: the
/// [`DownloadReport`] of a success, or the [`JsonError`] of a failure, for
/// the Job ID `job_id` or the Obs ID `obs_id` that was asked for.
fn print_download_line(
    target: DownloadTarget,
    result: &anyhow::Result<AsvoJob>,
    download_dir: &str,
) -> Result<(), anyhow::Error> {
    match result {
        Ok(job) => print_json_line(&DownloadReport::new(job, download_dir), true),
        Err(e) => {
            let line = match target {
                DownloadTarget::Job(job_id) => JsonError::new(e).with_job_id(job_id),
                DownloadTarget::Obs(obs_id) => JsonError::new(e).with_obs_id(obs_id),
            };
            print_json_line(&line, true)
        }
    }
}

/// What `download --dry-run --json` prints for each Job ID or Obs ID: the ID
/// and the download options.
#[derive(serde::Serialize)]
struct DryRunDownload {
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<AsvoJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    obs_id: Option<ObsId>,
    keep_tar: bool,
    no_resume: bool,
    skip_hash: bool,
}

/// What `submit-* --dry-run --json` prints for each Obs ID: the endpoint and
/// the request body.
#[derive(serde::Serialize)]
struct DryRunSubmission<'a, T: serde::Serialize> {
    endpoint: &'a str,
    obs_id: ObsId,
    params: &'a T,
}

/// What `cancel --dry-run --json` prints for each Job ID: the endpoint of
/// the DELETE request.
#[derive(serde::Serialize)]
struct DryRunCancel {
    endpoint: String,
    job_id: AsvoJobId,
}

/// What a successful download did, for its JSON report: the size and the
/// directory of an Acacia download, or the Scratch path that was moved to
/// the download directory.
fn downloaded_message(job: &AsvoJob, download_dir: &str) -> String {
    match job_files(job).first() {
        // A Scratch job that this host can reach is moved to the download
        // directory. (One that it cannot reach, or a DUG job, is an error.)
        Some(file) if file.type_ == FileType::Scratch => format!(
            "Moved {} to {}",
            file.path.as_deref().unwrap_or_default(),
            download_dir
        ),
        _ => format!("Downloaded {} to {}", job_size_text(job), download_dir),
    }
}

/// Submit one job per Obs ID, carrying on past failures.
///
/// One bad Obs ID does not stop the run. Each failure is reported as it happens,
/// the successes still go through, and the run ends with a summary. An
/// error is returned when anything failed, so the exit code still signals
/// it, but only after every Obs ID has been attempted. The error only counts
/// the failures, because each one was already reported. With `json`, each
/// failure is a [`JsonError`] line with the Obs ID.
fn submit_each_obs_id<F>(
    obs_ids: &[ObsId],
    description: &str,
    json: bool,
    mut submit: F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(&ObsId) -> Result<(), anyhow::Error>,
{
    let mut failures: usize = 0;

    for o in obs_ids {
        if let Err(e) = submit(o) {
            report_obs_id_error(&e, *o, json)?;
            failures += 1;
        }
    }

    let submitted = obs_ids.len() - failures;
    info!(
        "Submitted {} of {} Obs IDs for {}.",
        submitted,
        obs_ids.len(),
        description
    );

    if failures == 0 {
        return Ok(());
    }

    Err(ReportedFailures(format!(
        "{} of {} job submissions failed",
        failures,
        obs_ids.len()
    ))
    .into())
}

/// What a submit command was asked to do, for [`run_submit`].
struct Submission<'a> {
    /// The Obs IDs, one job each.
    obs_ids: &'a [ObsId],
    /// The endpoint, for `--dry-run`.
    endpoint: &'a str,
    /// The kind of job, for the summary, for example `conversion`.
    description: &'a str,
    /// `--wait`: wait for the jobs to be ready.
    wait: bool,
    /// `--dry-run`: send nothing.
    dry_run: bool,
    /// `--json`.
    json: bool,
}

/// Run a submit command: with `--dry-run`, report the body that `build`
/// makes for each Obs ID; otherwise log in, `submit` one job per Obs ID
/// (carrying on past failures, see [`submit_each_obs_id`]), and with
/// `--wait`, wait for the jobs that were submitted.
fn run_submit<P, B, S>(submission: &Submission, build: B, submit: S) -> anyhow::Result<()>
where
    P: serde::Serialize,
    B: Fn(ObsId) -> Result<P, AsvoApiError>,
    S: Fn(&AsvoClient, &P) -> Result<JobSubmittedResponse, AsvoApiError>,
{
    let Submission {
        obs_ids,
        endpoint,
        description,
        wait,
        dry_run,
        json,
    } = *submission;
    if dry_run {
        return report_dry_run_submissions(endpoint, obs_ids, json, build);
    }

    let client = connect()?;
    let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());
    let outcome = submit_each_obs_id(obs_ids, description, json, |o| {
        let params = build(*o)?;
        let resp = submit(&client, &params)?;
        print_json_line(&resp, json)?;
        info!("Submitted {} as MWA ASVO Job ID {}", o, resp.job_id);
        job_ids.push(resp.job_id);
        Ok(())
    });
    if wait {
        wait_loop(&client, &job_ids)?;
    }
    outcome
}

/// Report the error `e` of the Obs ID `obs_id`: a [`JsonError`] line with
/// `json`, a log record without.
fn report_obs_id_error(e: &anyhow::Error, obs_id: ObsId, json: bool) -> Result<(), anyhow::Error> {
    if json {
        print_json_line(&JsonError::new(e).with_obs_id(obs_id), true)
    } else {
        error!("Obs ID {}: {}", obs_id, e);
        Ok(())
    }
}

/// The message of `submit-image` for a Job ID: this command takes Obs IDs
/// only, and `submit-image-from-job` is the command for a conversion job.
const IMAGE_JOB_IDS_MESSAGE: &str = "This command only accepts Obs IDs; to image an existing conversion job, use submit-image-from-job instead.";

/// The message of `submit-image-from-job` for a Job ID among its arguments.
/// The command takes a conversion job, but as `--source-job-id`.
const IMAGE_FROM_JOB_JOB_IDS_MESSAGE: &str =
    "The arguments must be Obs IDs, not Job IDs. Give the conversion job with --source-job-id.";

/// Parse the Obs IDs of a submit command with the library's
/// [`parse_obs_ids_only`]. `job_ids_message` is the command's own text for
/// the error of a Job ID, or `None` for the library's text.
fn obs_ids_only(
    strings: &[String],
    job_ids_message: Option<&str>,
) -> Result<Vec<ObsId>, anyhow::Error> {
    parse_obs_ids_only(strings).map_err(|e| match (e, job_ids_message) {
        (ParseError::JobIdsGiven { .. }, Some(message)) => {
            ArgumentError(message.to_string()).into()
        }
        (e, _) => e.into(),
    })
}

/// Print a result as one line of JSON, for `--json`: the MWA ASVO's reply to
/// a submission or a cancellation, or the report of a download.
///
/// One compact object per job, in order, so a caller can read the Job IDs
/// back without parsing log text.
fn print_json_line(resp: &impl serde::Serialize, json: bool) -> Result<(), anyhow::Error> {
    if json {
        println!("{}", serde_json::to_string(resp)?);
    }
    Ok(())
}

/// Report what a submission would have sent, for `--dry-run`.
///
/// Every submit command prints the same thing: the endpoint the request
/// would go to, and the resolved JSON body for each Obs ID. The body is
/// built exactly as a real submission builds it, so a dry run exercises
/// the argument-to-request mapping rather than just echoing arguments.
/// With `json`, each is a [`DryRunSubmission`] line.
fn report_dry_run_submissions<T, F>(
    endpoint: &str,
    obs_ids: &[ObsId],
    json: bool,
    build_params: F,
) -> Result<(), anyhow::Error>
where
    T: serde::Serialize,
    F: Fn(ObsId) -> Result<T, AsvoApiError>,
{
    for o in obs_ids {
        let params = build_params(*o)?;
        if json {
            print_json_line(
                &DryRunSubmission {
                    endpoint,
                    obs_id: *o,
                    params: &params,
                },
                true,
            )?;
            continue;
        }
        info!(
            "[dry run] Would POST {} for Obs ID {}:\n{}",
            endpoint,
            o,
            serde_json::to_string_pretty(&params)?
        );
    }

    info!(
        "[dry run] Would have submitted {} Obs IDs to {}. Nothing was sent.",
        obs_ids.len(),
        endpoint
    );
    Ok(())
}

/// The log level for a `-v` count: none is `Info`, one is `Debug`, more is
/// `Trace`. With `--json` (`json`), no logs are printed: the level is `Off`
/// (clap refuses `-v` with `--json`).
fn log_level(verbosity: u8, json: bool) -> LevelFilter {
    match (json, verbosity) {
        (true, _) => LevelFilter::Off,
        (false, 0) => LevelFilter::Info,
        (false, 1) => LevelFilter::Debug,
        (false, _) => LevelFilter::Trace,
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
fn init_logger(verbosity: u8, json: bool) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in the logger")
        .build();
    let level = log_level(verbosity, json);
    if WriteLogger::init(level, log_config, std::io::stderr()).is_err() {
        log::set_max_level(level);
    }
}

fn init_logger_with_progressbar_support(level: u8, json: bool, multiprogressbar: &MultiProgress) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in the logger")
        .build();

    // To stderr, as in `init_logger`.
    let log = WriteLogger::new(log_level(level, json), log_config, std::io::stderr());

    // As in `init_logger`, a logger that is already installed is kept.
    if LogWrapper::new(multiprogressbar.clone(), log)
        .try_init()
        .is_err()
    {
        log::set_max_level(log_level(level, json));
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
            .filter_map(|id| jobs.0.iter().find(|j| j.job_id() == *id))
        {
            let log_prefix = format!("Job ID {} (Obs ID: {}):", job.job_id(), job.obs_id());
            match last_state.insert(job.job_id(), job.job_state) {
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
            init_logger(verbosity, json);

            let (job_ids, obs_ids) = parse_many_job_ids_or_obs_ids(&job_ids_or_obs_ids)?;
            let query = JobQuery {
                job_ids,
                obs_ids,
                job_types,
                job_states,
                filter: JobsFilter {
                    days,
                    date_from,
                    date_to,
                    sort_by,
                    ..JobsFilter::default()
                },
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
            json,
            ..
        } => {
            if job_ids_or_obs_ids.is_empty() {
                return Err(ArgumentError("No Job IDs or Obs IDs specified.".to_string()).into());
            }

            // Validate the download directory
            if !Path::new(&download_dir).exists() {
                return Err(ArgumentError(format!(
                    "Download directory `{download_dir}` does not exist or is not accessible."
                ))
                .into());
            }

            // Create progress bar capable of multiple downloads. With
            // --json, the bars are not drawn.
            let mpb = if json {
                MultiProgress::with_draw_target(ProgressDrawTarget::hidden())
            } else {
                MultiProgress::new()
            };

            // Init the logger- special case as we need to use LogWrapper to ensure log
            // messages don't mess up the progress bars!
            init_logger_with_progressbar_support(verbosity, json, &mpb);

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
                    debug!("Parsed Job IDs: {:#?}", job_ids);
                }
                if !obs_ids.is_empty() {
                    debug!("Parsed Obs IDs: {:#?}", obs_ids);
                }
                info!(
                    "Parsed {} Job IDs and {} Obs IDs for download. keep_tar={:?}, hash={:?}",
                    job_ids.len(),
                    obs_ids.len(),
                    keep_tar,
                    hash,
                );
                if json {
                    for target in DownloadTarget::all(&job_ids, &obs_ids) {
                        print_json_line(
                            &DryRunDownload {
                                job_id: target.job_id(),
                                obs_id: target.obs_id(),
                                keep_tar,
                                no_resume,
                                skip_hash,
                            },
                            true,
                        )?;
                    }
                }
            } else {
                let t: usize = job_ids.len() + obs_ids.len();
                // One client for all the downloads: it logs in once, and the
                // server permits only a few logins a minute.
                let client = connect()?;

                let targets = DownloadTarget::all(&job_ids, &obs_ids);
                let results: Vec<anyhow::Result<AsvoJob>> = targets
                    .par_iter()
                    .enumerate()
                    .map(|(c, target)| {
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
                            // Ctrl-C ends the CLI process.
                            should_stop: None,
                        };
                        target.download(&client, &opts)
                    })
                    .collect();

                // Every download runs to completion before anything is
                // reported, so one failure doesn't hide the rest. Report
                // each error, then fail the run as a whole so a script can
                // tell something went wrong.
                let mut failures = 0;
                for e in results.iter().filter_map(|r| r.as_ref().err()) {
                    error!("{e}");
                    failures += 1;
                }

                info!("Downloaded {} of {}.", t - failures, t);

                if json {
                    for (target, result) in targets.iter().zip(&results) {
                        print_download_line(*target, result, &download_dir)?;
                    }
                }

                if failures > 0 {
                    return Err(ReportedFailures(format!(
                        "{failures} of {t} downloads failed; see the errors above."
                    ))
                    .into());
                }
            }
        }

        Args::SubmitVis {
            download,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_DOWNLOAD_VIS_JOB,
                    description: "visibility download",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| download.to_vis_params(obs_id),
                |client, params| client.submit_download_vis_job(params),
            )?;
        }

        Args::SubmitConv {
            conv,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_CONVERSION_JOB,
                    description: "conversion",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| conv.to_params(obs_id),
                |client, params| client.submit_conversion_job(params),
            )?;
        }

        Args::SubmitImage {
            image,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_JOB_IDS_MESSAGE))?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_IMAGING_JOB,
                    description: "imaging",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| image.to_params(obs_id),
                |client, params| client.submit_imaging_job(params),
            )?;
        }

        Args::SubmitImageFromJob {
            image,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_FROM_JOB_JOB_IDS_MESSAGE))?;

            if obs_ids.len() != 1 {
                return Err(ArgumentError(
                    "submit-image-from-job requires exactly one obsid \
                     (the source_job_id identifies the conversion job for that obsid)."
                        .to_string(),
                )
                .into());
            }

            init_logger(verbosity, json);

            if dry_run {
                report_dry_run_submissions(ENDPOINT_IMAGE_FROM_JOB, &obs_ids, json, |obs_id| {
                    image.to_params(obs_id)
                })?;
            } else {
                let client = connect()?;

                let o = &obs_ids[0];
                let submit = || -> anyhow::Result<JobSubmittedResponse> {
                    let params = image.to_params(*o)?;
                    Ok(client.submit_image_from_job(&params)?)
                };
                let resp = match submit() {
                    Ok(resp) => resp,
                    Err(e) if json => {
                        report_obs_id_error(&e, *o, json)?;
                        return Err(ReportedFailures(e.to_string()).into());
                    }
                    Err(e) => return Err(e),
                };
                print_json_line(&resp, json)?;
                let job_id = resp.job_id;
                info!("Submitted {} as MWA ASVO Job ID {}", o, job_id);

                if wait {
                    wait_loop(&client, &[job_id])?;
                }
            }
        }

        Args::SubmitMeta {
            download,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_DOWNLOAD_VIS_JOB,
                    description: "metadata download",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| download.to_meta_params(obs_id),
                |client, params| client.submit_download_meta_job(params),
            )?;
        }

        Args::SubmitVolt {
            volt,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_VOLTAGE_JOB,
                    description: "voltage download",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| volt.to_params(obs_id),
                |client, params| client.submit_voltage_job(params),
            )?;
        }

        Args::SubmitBf {
            bf,
            submit,
            obs_ids,
        } => {
            let SubmitOptions {
                wait,
                dry_run,
                json,
                verbosity,
            } = submit;
            init_logger(verbosity, json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &Submission {
                    obs_ids: &obs_ids,
                    endpoint: ENDPOINT_BEAMFORMER_JOB,
                    description: "beamformer download",
                    wait,
                    dry_run,
                    json,
                },
                |obs_id| bf.to_params(obs_id),
                |client, params| client.submit_beamformer_job(params),
            )?;
        }

        Args::Wait {
            verbosity,
            jobs,
            json,
            legacy_json,
            no_colour,
        } => {
            let parsed_job_ids = parse_job_ids_only(&jobs)?;
            init_logger(verbosity, json);
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
            json,
            verbosity,
            jobs,
        } => {
            let parsed_job_ids = parse_job_ids_only(&jobs)?;
            init_logger(verbosity, json);

            if dry_run {
                for j in &parsed_job_ids {
                    info!("[dry run] Would DELETE {}/{}", ENDPOINT_JOBS, j);
                    let line = DryRunCancel {
                        endpoint: format!("{ENDPOINT_JOBS}/{j}"),
                        job_id: *j,
                    };
                    print_json_line(&line, json)?;
                }
                info!(
                    "[dry run] Would have cancelled {} Job IDs. Nothing was sent.",
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
                            print_json_line(&resp, json)?;
                        }
                        Err(e) => {
                            error!("Failed to cancel MWA ASVO Job ID {}: {}", j, e);
                            print_json_line(
                                &JsonError::new(&anyhow::Error::from(e)).with_job_id(j),
                                json,
                            )?;
                            failed_count += 1;
                        }
                    }
                }
                info!(
                    "Cancel requests: {} sent, {} failed.",
                    sent_count, failed_count
                );

                // A refusal with an HTTP error is a failure, so the run
                // fails, after every request was sent. (A job that is
                // already cancelled has a normal reply, which is not.)
                if failed_count > 0 {
                    return Err(ReportedFailures(format!(
                        "{failed_count} of {sent_count} cancel requests failed"
                    ))
                    .into());
                }
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
    let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).collect();
    let code = match Args::try_parse_from(&args) {
        Err(error) => {
            if is_usage_error(&error) && json_requested(&args) {
                print_json_error_line(&JsonError::usage(&error));
            } else {
                // Printing can fail (a closed pipe); the exit code is the same.
                let _ = error.print();
            }
            error.exit_code()
        }
        Ok(args) => {
            let json = args.json();
            match run(args) {
                Ok(()) => 0,
                Err(error) => {
                    if !json {
                        eprintln!("Error: {error:?}");
                    } else if error.downcast_ref::<ReportedFailures>().is_none() {
                        print_json_error_line(&JsonError::new(&error));
                    }
                    EXIT_CODE_FAILED
                }
            }
        }
    };
    // The process may end without Rust's own clean-up (it does when Python
    // ends it), so push out what is still held back.
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let _ = std::io::Write::flush(&mut std::io::stderr());
    code
}

/// Whether clap's `error` is an error of the arguments, not the help or the
/// version that clap prints by way of an error.
fn is_usage_error(error: &clap::Error) -> bool {
    use clap::error::ErrorKind;
    !matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// Whether the arguments `args`, which clap could not parse, ask for
/// `--json` (`-j`). clap parses them again and ignores the errors, so
/// `-vj` is found too.
fn json_requested(args: &[std::ffi::OsString]) -> bool {
    use clap::CommandFactory;
    Args::command()
        .ignore_errors(true)
        .try_get_matches_from(args)
        .ok()
        .and_then(|matches| {
            matches
                .subcommand()
                .and_then(|(_, sub)| sub.try_get_one::<bool>("json").ok().flatten().copied())
        })
        .unwrap_or(false)
}

/// Print the error line of `--json` on stdout. Printing can fail (a closed
/// pipe); the exit code is the same.
fn print_json_error_line(line: &JsonError) {
    if let Ok(text) = serde_json::to_string(line) {
        println!("{text}");
    }
}

impl Args {
    /// Whether the command was given `--json`.
    fn json(&self) -> bool {
        match self {
            Args::List { json, .. }
            | Args::Download { json, .. }
            | Args::Wait { json, .. }
            | Args::Cancel { json, .. } => *json,
            Args::SubmitVis { submit, .. }
            | Args::SubmitConv { submit, .. }
            | Args::SubmitImage { submit, .. }
            | Args::SubmitImageFromJob { submit, .. }
            | Args::SubmitMeta { submit, .. }
            | Args::SubmitVolt { submit, .. }
            | Args::SubmitBf { submit, .. } => submit.json,
        }
    }
}

/// The exit code of a command that failed. (clap's code for a bad argument
/// is 2.)
const EXIT_CODE_FAILED: i32 = 1;
