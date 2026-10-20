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
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use clap::Parser;
use jiff::Timestamp;
use log::{debug, error, info, warn};
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use indicatif_log_bridge::LogWrapper;

use crate::mwa_asvo::api::openapi::{JobSubmittedResponse, Status, Type as FileType};
use crate::mwa_asvo::api::validate::{
    validate_beamformer_params, validate_conversion_params, validate_download_params,
    validate_image_from_job_params, validate_imaging_params, validate_voltage_params,
};
use crate::mwa_asvo::{
    client_config_from_env, AsvoApiError, AsvoClient, AsvoJob, AsvoJobId, AsvoJobVec,
    DownloadOptions, DownloadProgress, DownloadSettings, JobState, JobsFilter,
    ENDPOINT_BEAMFORMER_JOB, ENDPOINT_CONVERSION_JOB, ENDPOINT_DOWNLOAD_VIS_JOB,
    ENDPOINT_IMAGE_FROM_JOB, ENDPOINT_IMAGING_JOB, ENDPOINT_JOBS, ENDPOINT_VOLTAGE_JOB,
};
use crate::obs_id::ObsId;
use crate::{
    parse_many_job_ids_or_obs_ids, parse_obs_ids_only, read_manta_ray_csv, CsvDefaults,
    CsvFileError, CsvJob, CsvJobsFailed, ParseError,
};

use super::json_output::{ArgumentError, JsonError, ReportedFailures, StatusFirst};
use super::legacy_json::{to_legacy_json, LEGACY_JSON_WARNING};
use super::stdin_ids::resolve_ids;
use super::table::{job_files, job_size_text, print_jobs_table};
use super::{Args, SubmitOptions, WAIT_INITIAL_DELAY, WAIT_POLL_INTERVAL, WAIT_RETRY_DURATION};

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

/// The time between the starts of two downloads, so that the downloads start
/// (and log) in their order: 1/20 before 2/20. It is for the display only.
/// A download that starts when another ends waits for it too.
const DOWNLOAD_START_DELAY: Duration = Duration::from_millis(50);

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

    /// Download the target, found in the job list `jobs`.
    fn download(
        self,
        client: &AsvoClient,
        jobs: &AsvoJobVec,
        opts: &DownloadOptions,
    ) -> anyhow::Result<AsvoJob> {
        Ok(match self {
            Self::Job(job_id) => client.download_job_from(jobs, job_id, opts)?,
            Self::Obs(obs_id) => client.download_obs_from(jobs, obs_id, opts)?,
        })
    }
}

/// The result of one successful download, for `download --json`: the keys
/// of the MWA ASVO's replies (`status` first, `job_id`, `message`), and the
/// Obs ID.
/// A failed download is a [`JsonError`] line.
#[derive(serde::Serialize)]
pub(super) struct DownloadReport {
    pub(super) status: Status,
    pub(super) job_id: AsvoJobId,
    pub(super) obs_id: ObsId,
    pub(super) message: String,
}

impl DownloadReport {
    /// The report of the download of `job`, to `download_dir` (with
    /// `job_subdir`, in the directory for the job below it).
    fn new(job: &AsvoJob, download_dir: &str, job_subdir: bool) -> Self {
        Self {
            status: Status::Success,
            job_id: job.job_id(),
            obs_id: job.obs_id(),
            message: downloaded_message(job, download_dir, job_subdir),
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
    job_subdir: bool,
) -> Result<(), anyhow::Error> {
    match result {
        Ok(job) => print_json_line(&DownloadReport::new(job, download_dir, job_subdir), true),
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
    /// Each job goes into a directory of its own, named for the Job ID.
    job_subdir: bool,
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
fn downloaded_message(job: &AsvoJob, download_dir: &str, job_subdir: bool) -> String {
    match job_files(job).first() {
        // A Scratch job that this host can reach is moved to the download
        // directory. (One that it cannot reach, or a DUG job, is an error.)
        // It is not put in a directory for the job: it is a folder of the
        // job already.
        Some(file) if file.type_ == FileType::Scratch => {
            let path = file.path.as_deref().unwrap_or_default();
            format!("Moved {path} to {download_dir}")
        }
        _ => {
            let size = job_size_text(job);
            let dir = if job_subdir {
                Path::new(download_dir)
                    .join(job.job_id().to_string())
                    .display()
                    .to_string()
            } else {
                download_dir.to_string()
            };
            format!("Downloaded {size} to {dir}")
        }
    }
}

/// Run `work` for each of `items` on the threads of `pool`, and give the
/// results in the order of the items.
///
/// The items start in their order: a thread that is free takes the next item
/// that has not started, and the starts are `start_delay` apart (for the
/// display: the first log line of an item comes before the start of the
/// next). `par_iter` would give each thread a block of the items, and the
/// second thread would start at the middle of the list: the log of the
/// downloads would say 1/20, 11/20, 6/20 and so on. `work` is given the
/// index of the item, from 0.
pub(super) fn run_in_order<T, R>(
    pool: &rayon::ThreadPool,
    items: &[T],
    start_delay: Duration,
    work: impl Fn(usize, &T) -> R + Sync,
) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    // The index of the next item to start. It is held for the wait, so that
    // another thread cannot start the next item before this one has.
    let next = Mutex::new(0_usize);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    pool.scope(|scope| {
        for _ in 0..pool.current_num_threads() {
            scope.spawn(|_| loop {
                let index = {
                    let mut next = next.lock().unwrap_or_else(PoisonError::into_inner);
                    if *next >= items.len() {
                        return;
                    }
                    let index = *next;
                    *next += 1;
                    thread::sleep(start_delay);
                    index
                };
                let result = work(index, &items[index]);
                done.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((index, result));
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, result)| result).collect()
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

    let total = obs_ids.len();
    let submitted = total - failures;
    info!("Submitted {submitted} of {total} Obs IDs for {description}.");

    if failures == 0 {
        return Ok(());
    }

    Err(ReportedFailures(format!("{failures} of {total} job submissions failed")).into())
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

impl SubmitOptions {
    /// What the command was asked to do, for [`run_submit`].
    fn submission<'a>(
        &self,
        obs_ids: &'a [ObsId],
        endpoint: &'a str,
        description: &'a str,
    ) -> Submission<'a> {
        Submission {
            obs_ids,
            endpoint,
            description,
            wait: self.wait,
            dry_run: self.dry_run,
            json: self.json,
        }
    }
}

/// Run a submit command. First `check` (the library's check of the request
/// body) checks the body that `build` makes, so that a bad body fails once,
/// before any login, also with `--dry-run`: the bodies differ only in their
/// Obs ID. Then, with `--dry-run`, report the body for each Obs ID;
/// otherwise log in, `submit` one job per Obs ID (carrying on past failures,
/// see [`submit_each_obs_id`]), and with `--wait`, wait for the jobs that
/// were submitted.
fn run_submit<P, B, C, S>(
    submission: &Submission,
    build: B,
    check: C,
    submit: S,
) -> anyhow::Result<()>
where
    P: serde::Serialize,
    B: Fn(ObsId) -> P,
    C: Fn(&P) -> Result<(), AsvoApiError>,
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
    if let Some(first) = obs_ids.first() {
        check(&build(*first))?;
    }
    if dry_run {
        return report_dry_run_submissions(endpoint, obs_ids, json, build);
    }

    let client = connect()?;
    let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(obs_ids.len());
    let outcome = submit_each_obs_id(obs_ids, description, json, |o| {
        let params = build(*o);
        let resp = submit(&client, &params)?;
        print_json_line(&StatusFirst(&resp), json)?;
        let job_id = resp.job_id;
        info!("Submitted {o} as MWA ASVO Job ID {job_id}");
        job_ids.push(job_id);
        Ok(())
    });
    if wait {
        wait_for_submitted(&client, &job_ids, outcome)
    } else {
        outcome
    }
}

/// Run `submit-csv`: read the jobs of a manta-ray-client CSV file, check all
/// of them, and submit them in the order of the file.
///
/// The whole file is read and checked before the login, so a mistake on any
/// line submits nothing, also with `--dry-run`. Then, as for the other submit
/// commands, a failed submission is reported when it happens and does not
/// stop the others ([`submit_each_csv_job`]), and with `--wait`, the command
/// waits for the jobs that were submitted.
fn run_submit_csv(submit: &SubmitOptions, allow_resubmit: bool, file: &Path) -> anyhow::Result<()> {
    let mut defaults = CsvDefaults::from_env()?;
    if allow_resubmit {
        defaults.allow_resubmit = Some(true);
    }
    let jobs = read_manta_ray_csv(file, &defaults).map_err(|e| match e {
        CsvFileError::Read { .. } => anyhow::Error::from(e),
        CsvFileError::Invalid(_) | CsvFileError::NoJobs { .. } => {
            ArgumentError(e.to_string()).into()
        }
    })?;

    if submit.dry_run {
        return report_dry_run_csv(&jobs, submit.json);
    }

    let client = connect()?;
    let mut job_ids: Vec<AsvoJobId> = Vec::with_capacity(jobs.len());
    let outcome = submit_each_csv_job(&client, &jobs, submit.json, &mut job_ids);
    if submit.wait {
        wait_for_submitted(&client, &job_ids, outcome)
    } else {
        outcome
    }
}

/// Submit the jobs of a CSV file with the library's
/// [`AsvoClient::submit_csv_jobs`], which carries on past failures, as
/// [`submit_each_obs_id`] does for the Obs IDs of a command line. The Job IDs
/// of the jobs that were submitted are added to `job_ids`. The error counts
/// the failures, because each was reported when it happened
/// ([`report_csv_result`]).
fn submit_each_csv_job(
    client: &AsvoClient,
    jobs: &[CsvJob],
    json: bool,
    job_ids: &mut Vec<AsvoJobId>,
) -> anyhow::Result<()> {
    // The callback cannot return an error: the first error of the output is
    // kept, and returned after the last job.
    let mut output_error: Option<anyhow::Error> = None;
    let result = client.submit_csv_jobs(jobs, |job, result| {
        if let Err(e) = report_csv_result(job, result, json) {
            output_error.get_or_insert(e);
        }
    });

    let total = jobs.len();
    let outcome = match result {
        Ok(responses) => {
            job_ids.extend(responses.iter().map(|r| r.job_id));
            Ok(())
        }
        Err(CsvJobsFailed {
            submitted,
            failures,
        }) => {
            job_ids.extend(submitted.iter().map(|r| r.job_id));
            Err(ReportedFailures(format!(
                "{} of {total} job submissions failed",
                failures.len()
            ))
            .into())
        }
    };
    info!("Submitted {} of {total} jobs.", job_ids.len());

    match output_error {
        Some(e) => Err(e),
        None => outcome,
    }
}

/// Report the result of one job of a CSV file: with `json`, the reply or a
/// [`JsonError`] line with the Obs ID; without, a log record that names the
/// line of the file.
fn report_csv_result(
    job: &CsvJob,
    result: &Result<JobSubmittedResponse, AsvoApiError>,
    json: bool,
) -> anyhow::Result<()> {
    match result {
        Ok(resp) => {
            print_json_line(&StatusFirst(resp), json)?;
            info!(
                "Line {}: submitted {} as MWA ASVO Job ID {}",
                job.line, job.obs_id, resp.job_id
            );
        }
        Err(e) if json => {
            print_json_line(&JsonError::api_error(e).with_obs_id(job.obs_id), true)?;
        }
        Err(e) => error!("Line {}: Obs ID {}: {e}", job.line, job.obs_id),
    }
    Ok(())
}

/// Report what `submit-csv --dry-run` would have sent: for each job, the
/// endpoint and the request body, as [`report_dry_run_submissions`] does.
/// With `json`, each is a [`DryRunSubmission`] line.
fn report_dry_run_csv(jobs: &[CsvJob], json: bool) -> anyhow::Result<()> {
    for job in jobs {
        let endpoint = job.params.endpoint();
        if json {
            print_json_line(
                &DryRunSubmission {
                    endpoint,
                    obs_id: job.obs_id,
                    params: &job.params,
                },
                true,
            )?;
            continue;
        }
        let body = serde_json::to_string_pretty(&job.params)?;
        info!(
            "[dry run] Would POST {endpoint} for Obs ID {} (line {}):\n{body}",
            job.obs_id, job.line
        );
    }

    info!(
        "[dry run] Would have submitted {} jobs. Nothing was sent.",
        jobs.len()
    );
    Ok(())
}

/// `--wait` of a submit command: wait for `job_ids`, the jobs that were
/// submitted, then return `outcome`, the result of the submissions. With
/// no job submitted, there is nothing to wait for. If the wait fails, its
/// error is returned, with the summary of the failed submissions (if any)
/// as its context, so that neither is lost.
pub(super) fn wait_for_submitted(
    client: &AsvoClient,
    job_ids: &[AsvoJobId],
    outcome: anyhow::Result<()>,
) -> anyhow::Result<()> {
    if job_ids.is_empty() {
        info!("No job was submitted, so there is nothing to wait for.");
        return outcome;
    }
    match (wait_loop(client, job_ids), outcome) {
        (Ok(()), outcome) => outcome,
        (Err(wait_error), Ok(())) => Err(wait_error),
        (Err(wait_error), Err(submit_error)) => {
            let summary = submit_error.to_string();
            Err(wait_error.context(summary))
        }
    }
}

/// Report the error `e` of the Obs ID `obs_id`: a [`JsonError`] line with
/// `json`, a log record without.
fn report_obs_id_error(e: &anyhow::Error, obs_id: ObsId, json: bool) -> Result<(), anyhow::Error> {
    if json {
        print_json_line(&JsonError::new(e).with_obs_id(obs_id), true)
    } else {
        error!("Obs ID {obs_id}: {e}");
        Ok(())
    }
}

/// The message of `download` when no Job ID or Obs ID is given, on the
/// command line or in a file.
const NO_DOWNLOAD_IDS_MESSAGE: &str = "No Job IDs or Obs IDs specified.";

/// What a command says when standard input (a pipe or a file) has no ID.
/// There is nothing to do, and that is not an error: the output of `list` is
/// empty when no job is in the state that `list --job-states` asked for.
const EMPTY_LIST_MESSAGE: &str = "Standard input has no IDs. Nothing to do.";

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
        let line = serde_json::to_string(resp)?;
        println!("{line}");
    }
    Ok(())
}

/// The `days` of `list`. clap refuses `--days` given with a date, so with a
/// date `days` is only its default, which the dates replace: the jobs are
/// selected by `days` or by the dates, not by both (the library refuses both).
pub(super) fn list_days(
    days: Option<NonZeroU64>,
    date_from: Option<Timestamp>,
    date_to: Option<Timestamp>,
) -> Option<NonZeroU64> {
    if date_from.is_some() || date_to.is_some() {
        None
    } else {
        days
    }
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
    F: Fn(ObsId) -> T,
{
    for o in obs_ids {
        let params = build_params(*o);
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
        let body = serde_json::to_string_pretty(&params)?;
        info!("[dry run] Would POST {endpoint} for Obs ID {o}:\n{body}");
    }

    let total = obs_ids.len();
    info!("[dry run] Would have submitted {total} Obs IDs to {endpoint}. Nothing was sent.");
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
    install_logger(verbosity, json, None);
}

/// As [`init_logger`], for a command that shows progress bars: the logger
/// writes through `multiprogressbar`, so that a log line does not break a
/// bar.
fn init_logger_with_progressbar_support(level: u8, json: bool, multiprogressbar: &MultiProgress) {
    install_logger(level, json, Some(multiprogressbar));
}

/// Install the logger (see [`init_logger`]), with `progress` if the command
/// has progress bars.
fn install_logger(verbosity: u8, json: bool, progress: Option<&MultiProgress>) {
    let log_config = ConfigBuilder::new()
        .set_time_offset_to_local()
        .expect("Unable to set time offset to local in the logger")
        .build();
    let level = log_level(verbosity, json);
    let installed = match progress {
        None => WriteLogger::init(level, log_config, std::io::stderr()).is_ok(),
        Some(multi) => LogWrapper::new(
            multi.clone(),
            WriteLogger::new(level, log_config, std::io::stderr()),
        )
        .try_init()
        .is_ok(),
    };
    // A logger that is already installed is kept; only the level changes.
    if !installed {
        log::set_max_level(level);
    }
}

/// Print `jobs` as `list` and `wait` do: as the legacy JSON of giant-squid
/// 2.x (`legacy_json`, with a warning), as JSON (`json`), or as a table.
/// Print the Job ID of each job, one on each line (`list --ids`). A list
/// with no job prints nothing.
fn print_job_ids(jobs: &AsvoJobVec) {
    if jobs.0.is_empty() {
        info!("You have no jobs.");
    }
    for job in &jobs.0 {
        println!("{}", job.job_id());
    }
}

/// The Job IDs of `cancel` and `wait`, from their arguments and from
/// standard input (see [`resolve_ids`]), and whether standard input was
/// read.
///
/// `None` means that the list on standard input has no job. There is
/// nothing to do, and that is not an error.
///
/// # Errors
///
/// [`ParseError`] if an ID is not valid, if there is an Obs ID, or if there
/// is no ID.
fn read_job_ids(args: &[String]) -> Result<Option<(Vec<AsvoJobId>, bool)>, ParseError> {
    let ids = resolve_ids(args)?;
    if ids.is_empty_input() {
        return Ok(None);
    }
    let from_stdin = ids.from_stdin;
    let job_ids = ids.into_job_ids()?;
    if job_ids.is_empty() {
        return Err(ParseError::NoJobIds);
    }
    Ok(Some((job_ids, from_stdin)))
}

/// Say how many Job IDs a command read from standard input, as `cancel` and
/// `wait` act on every one of them.
fn log_read_from_stdin(from_stdin: bool, job_count: usize) {
    if from_stdin {
        info!("Read {job_count} Job IDs from standard input.");
    }
}

fn print_jobs(
    jobs: AsvoJobVec,
    json: bool,
    legacy_json: bool,
    no_colour: bool,
) -> anyhow::Result<()> {
    if legacy_json {
        warn!("{LEGACY_JSON_WARNING}");
        let legacy = to_legacy_json(&jobs)?;
        println!("{legacy}");
    } else if json {
        let jobs_json = jobs.json()?;
        println!("{jobs_json}");
    } else {
        print_jobs_table(jobs, no_colour);
    }
    Ok(())
}

/// Poll the job list until all of `job_ids` are ready, logging each job's
/// state when it changes. Fails as soon as a job is missing, has an error,
/// or has been cancelled (see `AsvoJobVec::all_ready`).
fn wait_loop(client: &AsvoClient, job_ids: &[AsvoJobId]) -> anyhow::Result<()> {
    let job_count = job_ids.len();
    info!("Waiting for {job_count} jobs to be ready...");
    let mut last_state = BTreeMap::<AsvoJobId, JobState>::new();
    // Offer the MWA ASVO a kindness by waiting a moment, so that the
    // user's queue is hopefully current.
    std::thread::sleep(WAIT_INITIAL_DELAY);
    loop {
        // The default window of the API, as `list` without `--days` has.
        let jobs = client.get_jobs_retrying(&JobsFilter::default(), WAIT_RETRY_DURATION, None)?;
        let all_ready = jobs.all_ready(job_ids)?;

        // Log if there was a change in state. `all_ready` has already
        // checked that every job is in the list.
        for job in job_ids
            .iter()
            .filter_map(|id| jobs.0.iter().find(|j| j.job_id() == *id))
        {
            let job_id = job.job_id();
            let obs_id = job.obs_id();
            let state = job.job_state;
            let log_prefix = format!("Job ID {job_id} (Obs ID: {obs_id}):");
            match last_state.insert(job.job_id(), job.job_state) {
                Some(last_state) if last_state != job.job_state => {
                    info!("{log_prefix} is {state}");
                }
                Some(_) => (), // State did not change from last_state
                None => info!("{log_prefix} is {state}"), // First time just report current state
            }
        }

        if all_ready {
            break;
        }
        std::thread::sleep(WAIT_POLL_INTERVAL);
    }
    info!("All {job_count} MWA ASVO jobs are ready for download.");
    Ok(())
}

/// Run a command that clap has parsed.
fn run(args: Args) -> anyhow::Result<()> {
    match args {
        Args::List {
            verbosity,
            json,
            legacy_json,
            ids,
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
            let days = list_days(days, date_from, date_to);
            let filter = JobsFilter {
                days,
                date_from,
                date_to,
                sort_by,
                job_ids,
                obs_ids,
                job_types,
                job_states,
            };
            // Before connecting, so that a bad filter does not log in.
            filter.validate()?;
            let client = connect()?;
            let jobs = client.get_jobs(&filter)?;

            if ids {
                print_job_ids(&jobs);
            } else {
                print_jobs(jobs, json, legacy_json, no_colour)?;
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
            no_job_subdir,
            json,
            ..
        } => {
            let job_subdir = !no_job_subdir;
            // The IDs can come from files and from standard input, so they
            // are checked after they are read.
            let ids = resolve_ids(&job_ids_or_obs_ids)?;
            if ids.is_empty_input() {
                init_logger(verbosity, json);
                info!("{EMPTY_LIST_MESSAGE}");
                return Ok(());
            }
            if ids.is_empty() {
                return Err(ArgumentError(NO_DOWNLOAD_IDS_MESSAGE.to_string()).into());
            }
            let from_stdin = ids.from_stdin;
            let (job_ids, obs_ids) = (ids.job_ids, ids.obs_ids);

            if !Path::new(&download_dir).is_dir() {
                return Err(ArgumentError(format!(
                    "Download directory `{download_dir}` is not a directory, does not exist or is not accessible."
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
            if from_stdin {
                info!(
                    "Read {} Job IDs and {} Obs IDs from standard input.",
                    job_ids.len(),
                    obs_ids.len()
                );
            }

            let hash = !skip_hash;
            let DownloadSettings {
                buffer_size,
                retry_duration,
            } = DownloadSettings::from_env()?;
            if dry_run {
                if !job_ids.is_empty() {
                    debug!("Parsed Job IDs: {job_ids:#?}");
                }
                if !obs_ids.is_empty() {
                    debug!("Parsed Obs IDs: {obs_ids:#?}");
                }
                let job_count = job_ids.len();
                let obs_count = obs_ids.len();
                info!("Parsed {job_count} Job IDs and {obs_count} Obs IDs for download. keep_tar={keep_tar:?}, hash={hash:?}");
                if json {
                    for target in DownloadTarget::all(&job_ids, &obs_ids) {
                        print_json_line(
                            &DryRunDownload {
                                job_id: target.job_id(),
                                obs_id: target.obs_id(),
                                keep_tar,
                                no_resume,
                                skip_hash,
                                job_subdir,
                            },
                            true,
                        )?;
                    }
                }
            } else {
                let t: usize = job_ids.len() + obs_ids.len();
                // One client and one job list for all the downloads: the
                // client logs in once (the server permits only a few logins
                // a minute), and the job list is fetched once.
                let client = connect()?;
                let jobs = client.get_jobs(&JobsFilter::default())?;
                // A pool of its own, not rayon's global pool, which can be
                // set only once in a process.
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(concurrent_downloads)
                    .build()?;

                let targets = DownloadTarget::all(&job_ids, &obs_ids);
                let results: Vec<anyhow::Result<AsvoJob>> =
                    run_in_order(&pool, &targets, DOWNLOAD_START_DELAY, |index, target| {
                        let pb = create_progress_bar(&mpb);
                        let progress = |event| update_progress_bar(&pb, event);
                        let opts = DownloadOptions {
                            keep_tar,
                            no_resume,
                            hash,
                            download_dir: &download_dir,
                            job_subdirectory: job_subdir,
                            progress: Some(&progress),
                            download_number: index + 1,
                            download_count: t,
                            buffer_size,
                            retry_duration,
                            // Ctrl-C ends the CLI process.
                            should_stop: None,
                        };
                        target.download(&client, &jobs, &opts)
                    });

                // Every download runs to completion before anything is
                // reported, so one failure doesn't hide the rest. Report
                // each error, then fail the run as a whole so a script can
                // tell something went wrong.
                let mut failures = 0;
                for e in results.iter().filter_map(|r| r.as_ref().err()) {
                    error!("{e}");
                    failures += 1;
                }

                let downloaded = t - failures;
                info!("Downloaded {downloaded} of {t}.");

                if json {
                    for (target, result) in targets.iter().zip(&results) {
                        print_download_line(*target, result, &download_dir, job_subdir)?;
                    }
                }

                if failures > 0 {
                    return Err(
                        ReportedFailures(format!("{failures} of {t} downloads failed")).into(),
                    );
                }
            }
        }

        Args::SubmitVis {
            download,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_DOWNLOAD_VIS_JOB, "visibility download"),
                |obs_id| download.to_vis_params(obs_id),
                validate_download_params,
                |client, params| client.submit_download_vis_job(params),
            )?;
        }

        Args::SubmitConv {
            conv,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_CONVERSION_JOB, "conversion"),
                |obs_id| conv.to_params(obs_id),
                validate_conversion_params,
                |client, params| client.submit_conversion_job(params),
            )?;
        }

        Args::SubmitImage {
            image,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_JOB_IDS_MESSAGE))?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_IMAGING_JOB, "imaging"),
                |obs_id| image.to_params(obs_id),
                validate_imaging_params,
                |client, params| client.submit_imaging_job(params),
            )?;
        }

        Args::SubmitImageFromJob {
            image,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, Some(IMAGE_FROM_JOB_JOB_IDS_MESSAGE))?;

            if obs_ids.len() != 1 {
                return Err(ArgumentError(
                    "submit-image-from-job requires exactly one obsid \
                     (the source_job_id identifies the conversion job for that obsid)."
                        .to_string(),
                )
                .into());
            }

            run_submit(
                &submit.submission(
                    &obs_ids,
                    ENDPOINT_IMAGE_FROM_JOB,
                    "imaging from a conversion job",
                ),
                |obs_id| image.to_params(obs_id),
                validate_image_from_job_params,
                |client, params| client.submit_image_from_job(params),
            )?;
        }

        Args::SubmitMeta {
            download,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_DOWNLOAD_VIS_JOB, "metadata download"),
                |obs_id| download.to_meta_params(obs_id),
                validate_download_params,
                |client, params| client.submit_download_meta_job(params),
            )?;
        }

        Args::SubmitVolt {
            volt,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_VOLTAGE_JOB, "voltage download"),
                |obs_id| volt.to_params(obs_id),
                validate_voltage_params,
                |client, params| client.submit_voltage_job(params),
            )?;
        }

        Args::SubmitBf {
            bf,
            submit,
            obs_ids,
        } => {
            init_logger(submit.verbosity, submit.json);
            let obs_ids = obs_ids_only(&obs_ids, None)?;
            run_submit(
                &submit.submission(&obs_ids, ENDPOINT_BEAMFORMER_JOB, "beamformer download"),
                |obs_id| bf.to_params(obs_id),
                validate_beamformer_params,
                |client, params| client.submit_beamformer_job(params),
            )?;
        }

        Args::SubmitCsv {
            submit,
            allow_resubmit,
            file,
        } => {
            init_logger(submit.verbosity, submit.json);
            run_submit_csv(&submit, allow_resubmit, &file)?;
        }

        Args::Wait {
            verbosity,
            jobs,
            json,
            legacy_json,
            no_colour,
        } => {
            let Some((parsed_job_ids, from_stdin)) = read_job_ids(&jobs)? else {
                init_logger(verbosity, json);
                info!("{EMPTY_LIST_MESSAGE}");
                return Ok(());
            };
            init_logger(verbosity, json);
            log_read_from_stdin(from_stdin, parsed_job_ids.len());
            let client = connect()?;
            // Endlessly loop over the newly-supplied job IDs until
            // they're all ready.
            wait_loop(&client, &parsed_job_ids)?;

            let jobs = client.get_jobs(&JobsFilter {
                job_ids: parsed_job_ids,
                ..JobsFilter::default()
            })?;

            print_jobs(jobs, json, legacy_json, no_colour)?;
        }

        Args::Cancel {
            dry_run,
            json,
            verbosity,
            jobs,
        } => {
            let Some((parsed_job_ids, from_stdin)) = read_job_ids(&jobs)? else {
                init_logger(verbosity, json);
                info!("{EMPTY_LIST_MESSAGE}");
                return Ok(());
            };
            init_logger(verbosity, json);
            log_read_from_stdin(from_stdin, parsed_job_ids.len());

            if dry_run {
                for j in &parsed_job_ids {
                    info!("[dry run] Would DELETE {ENDPOINT_JOBS}/{j}");
                    let line = DryRunCancel {
                        endpoint: format!("{ENDPOINT_JOBS}/{j}"),
                        job_id: *j,
                    };
                    print_json_line(&line, json)?;
                }
                let job_count = parsed_job_ids.len();
                info!("[dry run] Would have cancelled {job_count} Job IDs. Nothing was sent.");
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
                            let message = &resp.message;
                            info!("Cancel request for Job ID {j}: {message}");
                            print_json_line(&StatusFirst(&resp), json)?;
                        }
                        Err(e) => {
                            error!("Failed to cancel MWA ASVO Job ID {j}: {e}");
                            print_json_line(
                                &JsonError::new(&anyhow::Error::from(e)).with_job_id(j),
                                json,
                            )?;
                            failed_count += 1;
                        }
                    }
                }
                info!("Cancel requests: {sent_count} sent, {failed_count} failed.");

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
            | Args::SubmitBf { submit, .. }
            | Args::SubmitCsv { submit, .. } => submit.json,
        }
    }
}

/// The exit code of a command that failed. (clap's code for a bad argument
/// is 2.)
const EXIT_CODE_FAILED: i32 = 1;
