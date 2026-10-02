# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The giant-squid commands. Each one calls the ``mwa_giant_squid`` module and prints the result."""

import argparse
import json
import logging
import os
import queue
import threading
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import mwa_giant_squid
from mwa_giant_squid import AsvoApiError, AsvoClient, AsvoError, AsvoJobVec, DownloadSettings

from .args import NON_PARAM_DESTS
from .constants import OBS_ID_HINT
from .progress import ProgressDisplay
from .table import print_jobs_table

log = logging.getLogger(__name__)

# How often the main thread looks for Ctrl-C while the download threads run, in seconds.
JOIN_POLL_S = 0.2


class UsageError(ValueError):
    """An option has a value that the MWA ASVO would refuse. The Rust command reports it as a usage error."""


# What a download or a submission can raise that is the user's problem, not a bug: it is reported as one line.
EXPECTED_ERRORS = (AsvoApiError, AsvoError, ValueError, OverflowError, OSError)


@dataclass(frozen=True)
class SubmitSpec:
    """How to submit one kind of job.

    Attributes:
        description: What the job is, for the summary line ("Submitted 2 of 2 obsids for <description>").
        endpoint: The MWA ASVO endpoint, which a dry run reports.
        params: The ``mwa_giant_squid`` function that builds the request body (and checks the arguments).
        method: The name of the ``AsvoClient`` method that submits the job.
    """

    description: str
    endpoint: str
    params: Callable[..., dict[str, Any]]
    method: str


SUBMIT_SPECS = {
    "submit-vis": SubmitSpec(
        "visibility download",
        mwa_giant_squid.ENDPOINT_DOWNLOAD_VIS_JOB,
        mwa_giant_squid.download_vis_job_params,
        "submit_download_vis_job",
    ),
    "submit-meta": SubmitSpec(
        "metadata download",
        mwa_giant_squid.ENDPOINT_DOWNLOAD_VIS_JOB,
        mwa_giant_squid.download_meta_job_params,
        "submit_download_meta_job",
    ),
    "submit-conv": SubmitSpec(
        "conversion",
        mwa_giant_squid.ENDPOINT_CONVERSION_JOB,
        mwa_giant_squid.conversion_job_params,
        "submit_conversion_job",
    ),
    "submit-image": SubmitSpec(
        "imaging", mwa_giant_squid.ENDPOINT_IMAGING_JOB, mwa_giant_squid.imaging_job_params, "submit_imaging_job"
    ),
    "submit-volt": SubmitSpec(
        "voltage download",
        mwa_giant_squid.ENDPOINT_VOLTAGE_JOB,
        mwa_giant_squid.voltage_job_params,
        "submit_voltage_job",
    ),
    "submit-bf": SubmitSpec(
        "beamformer download",
        mwa_giant_squid.ENDPOINT_BEAMFORMER_JOB,
        mwa_giant_squid.beamformer_job_params,
        "submit_beamformer_job",
    ),
}


def option_values(args: argparse.Namespace) -> dict[str, Any]:
    """Get the job options from a parsed command line.

    Args:
        args: The parsed arguments of a submit command.

    Returns:
        The options by name, which are the keyword arguments of the ``AsvoClient.submit_*`` methods.
    """
    return {name: value for name, value in vars(args).items() if name not in NON_PARAM_DESTS}


def obs_ids_only(strings: Sequence[str], job_ids_message: str | None = None) -> list[int]:
    """Parse obsids, refusing job IDs.

    Args:
        strings: The obsids and the paths of files of obsids.
        job_ids_message: The error message if a job ID is given. ``None`` lists the job IDs.

    Returns:
        The obsids.

    Raises:
        ValueError: There is a job ID, or there is no obsid.
    """
    job_ids, obs_ids = mwa_giant_squid.parse_many_job_ids_or_obs_ids(strings)
    if job_ids:
        raise ValueError(job_ids_message or f"Expected only obsids, but found these exceptions: {job_ids}")
    if not obs_ids:
        msg = "No obsids specified!"
        raise ValueError(msg)
    return obs_ids


def job_ids_only(strings: Sequence[str]) -> list[int]:
    """Parse job IDs, refusing obsids, as the Rust command does.

    An obsid is never ignored, even when job IDs are also given: that would wait for, or cancel, fewer jobs than
    the user asked for.

    Args:
        strings: The job IDs and the paths of files of job IDs.

    Returns:
        The job IDs.

    Raises:
        ValueError: There is an obsid, or there is no job ID.
    """
    job_ids, obs_ids = mwa_giant_squid.parse_many_job_ids_or_obs_ids(strings)
    if obs_ids:
        msg = f"Expected only job IDs, but found these obsids: {', '.join(map(str, obs_ids))}. {OBS_ID_HINT}"
        raise ValueError(msg)
    if not job_ids:
        msg = "No jobids specified!"
        raise ValueError(msg)
    return job_ids


def wait_loop(client: AsvoClient, job_ids: Sequence[int]) -> None:
    """Poll the job list until all of ``job_ids`` are ready, logging each job's state when it changes.

    Args:
        client: The client.
        job_ids: The jobs to wait for.

    Raises:
        AsvoError: A job is missing, has an error, has expired or has been cancelled.
    """
    log.info("Waiting for %d jobs to be ready...", len(job_ids))
    last_state: dict[int, Any] = {}
    # Wait a moment, so that the user's queue is hopefully current.
    time.sleep(mwa_giant_squid.WAIT_INITIAL_DELAY_SECS)
    while True:
        jobs = client.get_jobs()
        all_ready = jobs.all_ready(job_ids)
        by_id = {job.job_id: job for job in jobs}
        for job_id in job_ids:
            job = by_id[job_id]
            if job_id not in last_state or last_state[job_id] != job.job_state:
                log.info("Job ID %s (obsid: %s): is %s", job.job_id, job.obs_id, job.state_text)
            last_state[job_id] = job.job_state
        if all_ready:
            break
        time.sleep(mwa_giant_squid.WAIT_POLL_INTERVAL_SECS)
    log.info("All %d MWA ASVO jobs are ready for download.", len(job_ids))


def print_jobs(jobs: AsvoJobVec, as_json: bool, no_colour: bool) -> None:
    """Print jobs as JSON or as a table.

    Args:
        jobs: The jobs.
        as_json: Print JSON.
        no_colour: Do not colour the table.
    """
    if as_json:
        print(jobs.json())
    else:
        print_jobs_table(jobs, no_colour)


def cmd_list(args: argparse.Namespace) -> None:
    """Run ``list``.

    Args:
        args: The parsed arguments.

    Raises:
        ValueError: Both job IDs and obsids are given.
    """
    job_ids, obs_ids = mwa_giant_squid.parse_many_job_ids_or_obs_ids(args.job_ids_or_obs_ids)
    if job_ids and obs_ids:
        msg = "Invalid job_ids: can't specify both job IDs and obsids; use one or the other"
        raise ValueError(msg)
    client = AsvoClient.from_env()
    try:
        jobs = client.list_jobs(
            job_ids or None,
            obs_ids or None,
            args.job_types or None,
            args.job_states or None,
            days=args.days,
            date_from=args.date_from,
            date_to=args.date_to,
            sort_by=args.sort_by,
        )
    except ValueError as e:
        # The module checks the limits of the MWA ASVO (for example --days, 1 to 30) after the login. The Rust
        # command checks them first, and both report a usage error.
        raise UsageError(str(e)) from e
    print_jobs(jobs, args.json, args.no_colour)


def cmd_wait(args: argparse.Namespace) -> None:
    """Run ``wait``.

    Args:
        args: The parsed arguments.
    """
    job_ids = job_ids_only(args.jobs)
    client = AsvoClient.from_env()
    wait_loop(client, job_ids)
    print_jobs(client.list_jobs(job_ids), args.json, args.no_colour)


def cmd_cancel(args: argparse.Namespace) -> None:
    """Run ``cancel``.

    Args:
        args: The parsed arguments.
    """
    job_ids = job_ids_only(args.jobs)
    if args.dry_run:
        for job_id in job_ids:
            log.info("[dry run] Would DELETE %s/%s", mwa_giant_squid.ENDPOINT_JOBS, job_id)
        log.info("[dry run] Would have cancelled %d jobids. Nothing was sent.", len(job_ids))
        return
    client = AsvoClient.from_env()
    failed = 0
    for job_id in job_ids:
        try:
            response = client.cancel_job(job_id)
        except EXPECTED_ERRORS as e:
            log.error("Failed to cancel MWA ASVO job ID %s: %s", job_id, e)
            failed += 1
        else:
            # A reply is not proof that the job was cancelled: the server answers a job that is already
            # cancelled with a normal reply whose message says so. So the log reports requests.
            log.info("Cancel request for job %s: %s", job_id, response.message)
    log.info("Cancel requests: %d sent, %d failed.", len(job_ids), failed)


def build_body(params: Callable[..., dict[str, Any]], *args: Any, **options: Any) -> dict[str, Any]:
    """Build a request body, which checks the options against the limits of the MWA ASVO.

    Args:
        params: The ``mwa_giant_squid`` function that builds the body.
        *args: The positional arguments of ``params``.
        **options: The job options.

    Returns:
        The request body.

    Raises:
        UsageError: An option is not valid.
    """
    try:
        return params(*args, **options)
    except (ValueError, OverflowError) as e:
        raise UsageError(str(e)) from e


def print_submitted_json(response: Any, as_json: bool) -> None:
    """Print a submission's response as one line of JSON, for ``--json``.

    Args:
        response: The ``JobSubmittedResponse``.
        as_json: Print it.
    """
    if as_json:
        body = {"job_id": response.job_id, "message": response.message, "status": response.status}
        print(json.dumps(body, separators=(",", ":")))


def report_dry_run(spec: SubmitSpec, bodies: Sequence[tuple[int, dict[str, Any]]]) -> None:
    """Report what a submission would have sent, for ``--dry-run``.

    Args:
        spec: The kind of job.
        bodies: The obsid and the request body for each job.
    """
    for obs_id, body in bodies:
        log.info("[dry run] Would POST %s for obsid %s:\n%s", spec.endpoint, obs_id, json.dumps(body, indent=2))
    log.info("[dry run] Would have submitted %d obsids to %s. Nothing was sent.", len(bodies), spec.endpoint)


def cmd_submit(args: argparse.Namespace) -> None:
    """Run ``submit-vis``, ``submit-meta``, ``submit-conv``, ``submit-image``, ``submit-volt`` or ``submit-bf``.

    One job is submitted for each obsid. A failure does not stop the others: each is reported, and the run
    ends with a summary and an error if any failed.

    Args:
        args: The parsed arguments.

    Raises:
        ValueError: There is no obsid, or an option is not valid, or some of the submissions failed.
    """
    spec = SUBMIT_SPECS[args.command]
    if args.command == "submit-image":
        if not args.obs_ids:
            msg = "No obsids specified!"
            raise ValueError(msg)
        obs_ids = obs_ids_only(
            args.obs_ids,
            "This command only accepts obsids; to image an existing conversion job, use submit-image-from-job instead.",
        )
    else:
        obs_ids = obs_ids_only(args.obs_ids)
    options = option_values(args)
    # Build every request body first: this checks the options before the program logs in.
    bodies = [(obs_id, build_body(spec.params, obs_id, **options)) for obs_id in obs_ids]
    if args.dry_run:
        report_dry_run(spec, bodies)
        return

    client = AsvoClient.from_env()
    submit = getattr(client, spec.method)
    job_ids: list[int] = []
    failures: list[str] = []
    for obs_id in obs_ids:
        try:
            response = submit(obs_id, **options)
            print_submitted_json(response, args.json)
            log.info("Submitted %s as MWA ASVO job ID %s", obs_id, response.job_id)
            job_ids.append(response.job_id)
        except EXPECTED_ERRORS as e:
            log.error("Obsid %s: %s", obs_id, e)
            failures.append(f"{obs_id}: {e}")
    log.info("Submitted %d of %d obsids for %s.", len(obs_ids) - len(failures), len(obs_ids), spec.description)

    if args.wait:
        wait_loop(client, job_ids)
    if failures:
        joined = "\n  ".join(failures)
        msg = f"{len(failures)} of {len(obs_ids)} obsids failed:\n  {joined}"
        raise ValueError(msg)


def cmd_submit_image_from_job(args: argparse.Namespace) -> None:
    """Run ``submit-image-from-job``.

    Args:
        args: The parsed arguments.

    Raises:
        ValueError: There is not exactly one obsid, or an option is not valid.
    """
    if not args.obs_ids:
        msg = "No obsids specified!"
        raise ValueError(msg)
    job_ids, obs_ids = mwa_giant_squid.parse_many_job_ids_or_obs_ids(args.obs_ids)
    if job_ids:
        msg = "This command only accepts obsids, not job IDs."
        raise ValueError(msg)
    if len(obs_ids) != 1:
        msg = (
            "submit-image-from-job requires exactly one obsid "
            "(the source_job_id identifies the conversion job for that obsid)."
        )
        raise ValueError(msg)
    obs_id = obs_ids[0]
    options = option_values(args)
    body = build_body(mwa_giant_squid.image_from_job_params, obs_id, args.source_job_id, **options)
    if args.dry_run:
        spec = SubmitSpec(
            "imaging from a job", mwa_giant_squid.ENDPOINT_IMAGE_FROM_JOB, mwa_giant_squid.image_from_job_params, ""
        )
        report_dry_run(spec, [(obs_id, body)])
        return
    client = AsvoClient.from_env()
    response = client.submit_image_from_job(obs_id, args.source_job_id, **options)
    print_submitted_json(response, args.json)
    log.info("Submitted %s as MWA ASVO image-from-job ID %s", obs_id, response.job_id)
    if args.wait:
        wait_loop(client, [response.job_id])


@dataclass(frozen=True)
class DownloadTask:
    """One download.

    Attributes:
        is_job_id: ``True`` if ``number`` is a job ID, ``False`` if it is an obsid.
        number: The job ID or the obsid.
    """

    is_job_id: bool
    number: int


def run_downloads(
    client: AsvoClient,
    tasks: Sequence[DownloadTask],
    display: ProgressDisplay,
    concurrency: int,
    download_dir: Path,
    options: dict[str, Any],
) -> list[BaseException | None]:
    """Run the downloads, up to ``concurrency`` at a time.

    A single download, or ``concurrency`` of 1, runs in the main thread, so that Ctrl-C stops it at the next
    chunk and a partial file stays to be resumed. Otherwise the downloads run in daemon threads, and Ctrl-C
    ends the program (the module stops a download on Ctrl-C only in the main thread).

    Args:
        client: The client, which all the downloads share.
        tasks: The downloads.
        display: The progress bars.
        concurrency: How many downloads to run at once.
        download_dir: The directory for the files.
        options: The keyword arguments of ``download_job`` that are the same for each download.

    Returns:
        The error of each download, or ``None`` if it worked.
    """
    results: list[BaseException | None] = [None] * len(tasks)

    def run_one(index: int) -> None:
        task = tasks[index]
        bar = display.add_bar()
        download = client.download_job if task.is_job_id else client.download_obs
        try:
            download(
                task.number,
                download_dir,
                progress=bar.update,
                download_number=index + 1,
                download_count=len(tasks),
                **options,
            )
        except EXPECTED_ERRORS as e:
            results[index] = e

    if concurrency <= 1 or len(tasks) == 1:
        for index in range(len(tasks)):
            run_one(index)
        return results

    pending: queue.Queue[int] = queue.Queue()
    for index in range(len(tasks)):
        pending.put(index)

    def worker() -> None:
        while True:
            try:
                index = pending.get_nowait()
            except queue.Empty:
                return
            run_one(index)

    threads = [threading.Thread(target=worker, daemon=True) for _ in range(min(concurrency, len(tasks)))]
    for thread in threads:
        thread.start()
    # Wait with a timeout, so that Ctrl-C reaches the main thread.
    while any(thread.is_alive() for thread in threads):
        for thread in threads:
            thread.join(JOIN_POLL_S)
    return results


def cmd_download(args: argparse.Namespace, display: ProgressDisplay) -> None:
    """Run ``download``.

    Args:
        args: The parsed arguments.
        display: The progress bars.

    Raises:
        ValueError: There is nothing to download, the directory does not exist, or some downloads failed.
    """
    if not args.job_ids_or_obs_ids:
        msg = "No jobs or obsids specified!"
        raise ValueError(msg)
    if not Path(args.download_dir).exists():
        msg = f"Download directory `{args.download_dir}` does not exist or is not accessible."
        raise ValueError(msg)
    concurrency = args.concurrent_downloads if args.concurrent_downloads > 0 else (os.cpu_count() or 1)
    job_ids, obs_ids = mwa_giant_squid.parse_many_job_ids_or_obs_ids(args.job_ids_or_obs_ids)
    hash_files = not args.skip_hash
    settings = DownloadSettings.from_env()
    if args.dry_run:
        if job_ids:
            log.debug("Parsed job IDs: %s", job_ids)
        if obs_ids:
            log.debug("Parsed obsids: %s", obs_ids)
        log.info(
            "Parsed %d jobids and %d obsids for download. keep_tar=%s, hash=%s",
            len(job_ids),
            len(obs_ids),
            str(args.keep_tar).lower(),
            str(hash_files).lower(),
        )
        return

    tasks = [DownloadTask(True, j) for j in job_ids] + [DownloadTask(False, o) for o in obs_ids]
    client = AsvoClient.from_env()
    options = {
        "keep_tar": args.keep_tar,
        "no_resume": args.no_resume,
        "hash": hash_files,
        "buffer_size": settings.buffer_size,
        "retry_duration": settings.retry_duration,
    }
    results = run_downloads(client, tasks, display, concurrency, Path(args.download_dir), options)
    display.close()
    # Every download has run to completion before anything is reported, so one failure does not hide the rest.
    failures = [error for error in results if error is not None]
    for error in failures:
        log.error("%s", error)
    log.info("Downloaded %d of %d.", len(tasks) - len(failures), len(tasks))
    if failures:
        msg = f"{len(failures)} of {len(tasks)} downloads failed; see the errors above."
        raise ValueError(msg)
