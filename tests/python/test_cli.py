"""Tests for the ``giant-squid`` command line program, against a local mock MWA ASVO.

The program runs in this process (``main`` is called with the arguments), with the environment set by the
fixtures. A few tests start it in a subprocess, as a user would.
"""

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest
from pytest_httpserver import HTTPServer
from werkzeug import Request, Response

import mwa_giant_squid as gs
import mwa_giant_squid_cli
from mwa_giant_squid_cli import commands as cli_commands

from .conftest import GET_JOBS_PATH, TEST_API_KEY, TEST_OBS_ID, error_response, job_detail
from .test_download import FILE_PATH, JOB_ID, MEMBER_CONTENTS, MEMBER_NAME, ready_job, tar_bytes

# The exit codes of the program.
EXIT_OK = 0
EXIT_FAILED = 1
EXIT_USAGE = 2
EXIT_INTERRUPTED = 130

# The endpoints that the program uses.
CONVERSION_PATH = "/api/v2/conversion_job"
DOWNLOAD_PATH = "/api/v2/download_vis_job"
IMAGING_PATH = "/api/v2/imaging_job"
IMAGE_FROM_JOB_PATH = "/api/v2/image_from_job"
VOLTAGE_PATH = "/api/v2/voltage_job"
BEAMFORMER_PATH = "/api/v2/beamformer_job"
JOBS_PATH = "/api/v2/jobs"

# Jobs and obsids that the mock knows.
LISTED_JOB_ID = 31
QUEUED_JOB_ID = 32
FAILED_JOB_ID = 33
OTHER_OBS_ID = 1065880248
THIRD_OBS_ID = 1065880368
SUBMITTED_JOB_ID = 777
FAILED_ERROR_CODE = 7
SOURCE_JOB_ID = 555

# One GiB and a half, as bytes, and how the table shows it.
FILE_SIZE = 1610612736
FILE_SIZE_TEXT = "1.5 GiB"

# How long a subprocess may take, and how long to let a download run before pressing Ctrl-C, in seconds.
SUBPROCESS_TIMEOUT_S = 60
SIGINT_DELAY_S = 1.5
MAX_STOP_TIME_S = 15

# The slow file for the Ctrl-C test: chunks of CHUNK_SIZE bytes, one every CHUNK_DELAY_S.
CHUNKS = 400
CHUNK_SIZE = 1024
CHUNK_DELAY_S = 0.05
SECOND_JOB_ID = JOB_ID + 1
SECOND_FILE_PATH = "/downloads/second.tar"


@dataclass
class Result:
    """What a run of the program produced.

    Attributes:
        code: The exit code.
        out: What it wrote to standard output.
        err: What it wrote to standard error.
    """

    code: int
    out: str
    err: str


@pytest.fixture
def cli_env(host: str, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Point the program at the mock, with an empty home directory.

    Args:
        host: The mock's base URL.
        tmp_path: A directory for the home directory.
        monkeypatch: Sets the environment.

    Returns:
        The home directory.
    """
    for name in list(os.environ):
        if name.startswith(("MWA_ASVO_", "GIANT_SQUID_")):
            monkeypatch.delenv(name)
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("MWA_ASVO_HOST", host)
    monkeypatch.setenv("MWA_ASVO_API_KEY", TEST_API_KEY)
    return tmp_path


@pytest.fixture
def run(cli_env: Path, capsys: pytest.CaptureFixture[str]) -> Callable[..., Result]:
    """A function that runs the program with arguments.

    Args:
        cli_env: Sets the environment.
        capsys: Captures the output.

    Returns:
        The function. A usage error (``SystemExit``) is a result with its exit code.
    """

    def run_cli(*args: str) -> Result:
        try:
            code = mwa_giant_squid_cli.main(list(args))
        except SystemExit as e:
            code = e.code if isinstance(e.code, int) else EXIT_FAILED
        captured = capsys.readouterr()
        return Result(code, captured.out, captured.err)

    return run_cli


@pytest.fixture
def no_sleep(monkeypatch: pytest.MonkeyPatch) -> list[float]:
    """Make the wait loop's sleeps instant.

    Args:
        monkeypatch: Replaces the sleep.

    Returns:
        The list that receives the length of each sleep.
    """
    sleeps: list[float] = []
    monkeypatch.setattr(cli_commands.time, "sleep", sleeps.append)
    return sleeps


@pytest.fixture
def three_jobs(mock_login: None, serve_jobs: Callable[[list[dict[str, Any]]], None]) -> None:
    """Serve a ready job, a queued conversion job and a failed job.

    Args:
        mock_login: Serves the login.
        serve_jobs: Serves the job list.
    """
    product = {"files": [{"type": "acacia", "url": "http://example.org/a.tar", "size": FILE_SIZE, "sha1": "ab"}]}
    serve_jobs(
        [
            job_detail(LISTED_JOB_ID, "completed", product=product, completed="2026-09-08T06:00:00"),
            job_detail(
                QUEUED_JOB_ID,
                "queued",
                job_type=0,
                job_params={"obs_id": str(OTHER_OBS_ID)},
            ),
            job_detail(
                FAILED_JOB_ID,
                "error",
                error_code=FAILED_ERROR_CODE,
                error_text="boom",
                job_params={"obs_id": str(THIRD_OBS_ID)},
            ),
        ]
    )


def serve_submission(httpserver: HTTPServer, path: str) -> None:
    """Serve a successful submission at ``path``.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.
    """
    httpserver.expect_request(path, method="POST").respond_with_json(
        {"job_id": SUBMITTED_JOB_ID, "message": "Job submitted", "status": "success"}
    )


def bodies_sent_to(httpserver: HTTPServer, path: str) -> list[dict[str, Any]]:
    """The JSON bodies of the requests that the mock received at ``path``.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.

    Returns:
        The bodies, in order.
    """
    return [json.loads(request.get_data()) for request, _ in httpserver.log if request.path == path]


def log_lines(result: Result) -> list[str]:
    """The log lines on standard error, without the time.

    Args:
        result: The result of a run.

    Returns:
        The lines, for example ``[INFO] Cancelled 2 jobs.``.
    """
    return [re.sub(r"^\d\d:\d\d:\d\d ", "", line) for line in result.err.splitlines()]


# list


def test_list_prints_a_table_like_the_rust_command(run: Callable[..., Result], three_jobs: None) -> None:
    """The table has the Rust command's columns, a row for each job, and a job's error message in its state."""
    result = run("list")

    assert result.code == EXIT_OK, result.err
    rule = "+" + "+".join("-" * width for width in (8, 12, 23, 13, 11, 10, 18)) + "+"
    lines = result.out.splitlines()
    assert lines[0] == lines[2] == lines[-1] == rule
    assert lines[1] == (
        "| Job ID | Obsid      | Job Type              | Job State   | File Size | Delivery | Completed        |"
    )
    assert lines[3] == (
        f"| {LISTED_JOB_ID}     | {TEST_OBS_ID} | Download Visibilities | Ready       | {FILE_SIZE_TEXT}   "
        "| acacia   | 2026-09-08 06:00 |"
    )
    assert "| Queued      |" in lines[4]
    assert "| Error: boom |" in lines[5]


def test_list_says_when_there_are_no_jobs(
    run: Callable[..., Result], mock_login: None, serve_jobs: Callable[[list[dict[str, Any]]], None]
) -> None:
    """With no jobs the program prints a message and not an empty table."""
    serve_jobs([])

    result = run("list")

    assert result.out == "You have no jobs.\n"


def test_list_json_is_the_library_json(run: Callable[..., Result], three_jobs: None) -> None:
    """``--json`` prints the jobs as the library does, keyed by job ID."""
    result = run("list", "--json")

    assert result.code == EXIT_OK, result.err
    jobs = json.loads(result.out)
    assert list(jobs) == [str(LISTED_JOB_ID), str(QUEUED_JOB_ID), str(FAILED_JOB_ID)]
    assert jobs[str(FAILED_JOB_ID)]["error_text"] == "boom"
    assert jobs[str(FAILED_JOB_ID)]["error_code"] == FAILED_ERROR_CODE
    assert jobs[str(LISTED_JOB_ID)]["error_code"] is None


@pytest.mark.parametrize(
    ("filters", "expected"),
    [
        (["--job-states", "ready,error"], [LISTED_JOB_ID, FAILED_JOB_ID]),
        (["--states", "Queued"], [QUEUED_JOB_ID]),
        (["--job-types", "Conversion"], [QUEUED_JOB_ID]),
        (["--types", "download_visibilities"], [LISTED_JOB_ID, FAILED_JOB_ID]),
        ([str(LISTED_JOB_ID), str(FAILED_JOB_ID)], [LISTED_JOB_ID, FAILED_JOB_ID]),
        ([str(OTHER_OBS_ID)], [QUEUED_JOB_ID]),
    ],
)
def test_list_filters(run: Callable[..., Result], three_jobs: None, filters: list[str], expected: list[int]) -> None:
    """The list is filtered by state, type, job ID and obsid."""
    result = run("list", "--json", *filters)

    assert result.code == EXIT_OK, result.err
    assert sorted(map(int, json.loads(result.out))) == expected


def test_list_refuses_job_ids_and_obsids_together(run: Callable[..., Result], three_jobs: None) -> None:
    """Job IDs and obsids cannot be mixed, as in the Rust command."""
    result = run("list", str(LISTED_JOB_ID), str(TEST_OBS_ID))

    assert result.code == EXIT_FAILED
    assert result.err.startswith("Error: Invalid job_ids: can't specify both job IDs and obsids")


@pytest.mark.parametrize(
    ("option", "value"),
    [
        ("--date-from", "2026-09-01T10:00:00"),
        ("--date-to", "yesterday"),
        ("--job-states", "bogus"),
        ("--job-types", "bogus"),
        ("--days", "many"),
    ],
)
def test_list_refuses_a_bad_value_with_a_usage_error(
    run: Callable[..., Result], three_jobs: None, option: str, value: str
) -> None:
    """A value that cannot be parsed is a usage error, before any request."""
    result = run("list", option, value)

    assert result.code == EXIT_USAGE
    assert option in result.err


@pytest.mark.parametrize("days", ["0", "31"])
def test_list_days_outside_one_to_thirty_is_a_usage_error(
    run: Callable[..., Result], three_jobs: None, days: str
) -> None:
    """The module refuses ``--days`` outside the schema's 1 to 30, and the program reports it as a usage error."""
    result = run("list", "--days", days)

    assert result.code == EXIT_USAGE
    assert "error: Invalid days: must be between 1 and 30" in result.err


def test_list_accepts_dates_and_times(run: Callable[..., Result], three_jobs: None) -> None:
    """A date, and an RFC 3339 time with ``Z`` or an offset, are accepted."""
    result = run("list", "-n", "--date-from", "2026-09-01", "--date-to", "2026-09-30T00:00:00Z")

    assert result.code == EXIT_OK, result.err
    assert run("list", "--date-from", "2026-09-01T00:00:00+08:00").code == EXIT_OK


def test_the_short_command_name_works(run: Callable[..., Result], three_jobs: None) -> None:
    """``l`` is ``list``."""
    assert run("l", "-j").out == run("list", "-j").out


# submit


# Each submit command, with the extra arguments it needs, the endpoint it uses and the function that builds the
# request body that the program must send.
SUBMIT_CASES = [
    ("submit-vis", [], DOWNLOAD_PATH, lambda o: gs.download_vis_job_params(o)),
    ("submit-meta", [], DOWNLOAD_PATH, lambda o: gs.download_meta_job_params(o)),
    ("submit-conv", [], CONVERSION_PATH, lambda o: gs.conversion_job_params(o)),
    ("submit-image", [], IMAGING_PATH, lambda o: gs.imaging_job_params(o)),
    (
        "submit-image-from-job",
        ["--source-job-id", str(SOURCE_JOB_ID)],
        IMAGE_FROM_JOB_PATH,
        lambda o: gs.image_from_job_params(o, SOURCE_JOB_ID),
    ),
    ("submit-volt", ["--offset", "10", "--duration", "32"], VOLTAGE_PATH, lambda o: gs.voltage_job_params(o, 10, 32)),
    ("submit-bf", [], BEAMFORMER_PATH, lambda o: gs.beamformer_job_params(o)),
]


@pytest.mark.parametrize(("command", "extra", "path", "build"), SUBMIT_CASES)
def test_a_dry_run_reports_the_default_body_and_sends_nothing(
    run: Callable[..., Result],
    httpserver: HTTPServer,
    command: str,
    extra: list[str],
    path: str,
    build: Callable[[int], dict[str, Any]],
) -> None:
    """``--dry-run`` logs the endpoint and the body with the MWA ASVO defaults, and makes no request."""
    result = run(command, "--dry-run", *extra, str(TEST_OBS_ID))

    assert result.code == EXIT_OK, result.err
    lines = log_lines(result)
    assert lines[0] == f"[INFO] [dry run] Would POST {path} for obsid {TEST_OBS_ID}:"
    body = json.loads("\n".join(lines[1 : lines.index("}") + 1]))
    assert body == build(TEST_OBS_ID)
    assert lines[-1] == f"[INFO] [dry run] Would have submitted 1 obsids to {path}. Nothing was sent."
    assert httpserver.log == []


@pytest.mark.parametrize(("command", "extra", "path", "build"), SUBMIT_CASES)
def test_a_submission_sends_the_default_body(
    run: Callable[..., Result],
    httpserver: HTTPServer,
    mock_login: None,
    command: str,
    extra: list[str],
    path: str,
    build: Callable[[int], dict[str, Any]],
) -> None:
    """Each submit command sends the same body that its dry run reports, and prints the job ID."""
    serve_submission(httpserver, path)

    result = run(command, *extra, str(TEST_OBS_ID), "--json")

    assert result.code == EXIT_OK, result.err
    assert bodies_sent_to(httpserver, path) == [build(TEST_OBS_ID)]
    assert result.out == '{"job_id":777,"message":"Job submitted","status":"success"}\n'
    verb = "image-from-job" if command == "submit-image-from-job" else "job"
    assert f"[INFO] Submitted {TEST_OBS_ID} as MWA ASVO {verb} ID {SUBMITTED_JOB_ID}" in log_lines(result)


@pytest.mark.parametrize(
    ("alias", "command"),
    [("sv", "submit-vis"), ("sc", "submit-conv"), ("si", "submit-image"), ("sm", "submit-meta"), ("sb", "submit-bf")],
)
def test_the_short_command_names_work(run: Callable[..., Result], alias: str, command: str) -> None:
    """The short names are the commands."""
    assert (
        run(alias, "-n", str(TEST_OBS_ID)).err.split("\n", 1)[1:]
        == run(command, "-n", str(TEST_OBS_ID)).err.split("\n", 1)[1:]
    )


def test_the_conversion_options_are_sent(run: Callable[..., Result], httpserver: HTTPServer, mock_login: None) -> None:
    """The options of ``submit-conv`` change the body, including a negative declination."""
    serve_submission(httpserver, CONVERSION_PATH)

    result = run(
        "submit-conv", "-o", "uvfits", "-d", "dug", "-f", "tar", "--avg-freq-res", "80", "--centre", "custom",
        "--custom-centre-ra", "12.5", "--custom-centre-dec", "-26.7", "--apply-di-cal", "--no-rfi", "-r",
        str(TEST_OBS_ID),
    )  # fmt: skip

    assert result.code == EXIT_OK, result.err
    (body,) = bodies_sent_to(httpserver, CONVERSION_PATH)
    assert body == gs.conversion_job_params(
        TEST_OBS_ID,
        output=gs.Output.Uvfits,
        delivery=gs.Delivery.Dug,
        delivery_format=gs.DeliveryFormat.Tar,
        avg_freq_res=80.0,
        centre=gs.Centre.Custom,
        custom_centre_ra=12.5,
        custom_centre_dec=-26.7,
        apply_di_cal=True,
        no_rfi=True,
        allow_resubmit=True,
    )


def test_a_boolean_option_takes_a_value_only_after_an_equals_sign(run: Callable[..., Result]) -> None:
    """``--join-channels`` before an obsid is a flag and leaves the obsid alone; ``=false`` sets it false."""
    result = run("submit-image", "-n", "--join-channels", "--apply-di-cal=false", str(TEST_OBS_ID), str(OTHER_OBS_ID))

    assert result.code == EXIT_OK, result.err
    assert f"Would POST {IMAGING_PATH} for obsid {TEST_OBS_ID}" in result.err
    assert f"Would POST {IMAGING_PATH} for obsid {OTHER_OBS_ID}" in result.err
    assert '"join_channels": true' in result.err
    assert '"apply_di_cal": false' in result.err


def test_the_delivery_comes_from_the_environment(run: Callable[..., Result], monkeypatch: pytest.MonkeyPatch) -> None:
    """``GIANT_SQUID_DELIVERY`` and ``GIANT_SQUID_DELIVERY_FORMAT`` set the defaults, and an option overrides them."""
    monkeypatch.setenv("GIANT_SQUID_DELIVERY", "dug")
    monkeypatch.setenv("GIANT_SQUID_DELIVERY_FORMAT", "files")

    result = run("submit-vis", "-n", str(TEST_OBS_ID))
    overridden = run("submit-vis", "-n", "-d", "scratch", str(TEST_OBS_ID))

    assert '"delivery": "dug"' in result.err
    assert '"delivery_format": "files"' in result.err
    assert '"delivery": "scratch"' in overridden.err


def test_a_bad_delivery_in_the_environment_is_a_usage_error(
    run: Callable[..., Result], monkeypatch: pytest.MonkeyPatch
) -> None:
    """A delivery that is not one of the choices is refused, whether it comes from the command line or not."""
    monkeypatch.setenv("GIANT_SQUID_DELIVERY", "moon")

    result = run("submit-vis", "-n", str(TEST_OBS_ID))

    assert result.code == EXIT_USAGE
    assert "moon" in result.err


@pytest.mark.parametrize(
    ("args", "message"),
    [
        (["submit-conv", "--avg-freq-res", "5000"], "avg_freq_res: must be between 0 and 1280"),
        (["submit-volt", "-o", "9000", "-u", "32"], "offset: must be between 0 and 5400"),
        (["submit-image", "--image-size", "100"], "image_size"),
    ],
)
def test_an_option_that_is_out_of_range_is_a_usage_error_before_any_request(
    run: Callable[..., Result], httpserver: HTTPServer, args: list[str], message: str
) -> None:
    """The limits of the MWA ASVO are checked before the program logs in, and exit with 2 as in the Rust command."""
    result = run(*args, str(TEST_OBS_ID))

    assert result.code == EXIT_USAGE
    assert message in result.err
    assert httpserver.log == []


@pytest.mark.parametrize(
    ("args", "message"),
    [
        (["submit-vis"], "No obsids specified!"),
        (["submit-image"], "No obsids specified!"),
        (["submit-vis", "123"], "Expected only obsids, but found these exceptions: [123]"),
        (["submit-image", "123"], "use submit-image-from-job instead"),
        (["submit-image-from-job", "--source-job-id", "5", "123"], "only accepts obsids, not job IDs"),
        (
            ["submit-image-from-job", "--source-job-id", "5", str(TEST_OBS_ID), str(OTHER_OBS_ID)],
            "requires exactly one obsid",
        ),
    ],
)
def test_the_obsids_are_checked(run: Callable[..., Result], args: list[str], message: str) -> None:
    """The messages for a missing obsid, a job ID and a second obsid are the Rust command's."""
    result = run(*args)

    assert result.code == EXIT_FAILED
    assert message in result.err


def test_missing_required_options_are_usage_errors(run: Callable[..., Result]) -> None:
    """``submit-volt`` needs ``--offset`` and ``--duration``; ``submit-image-from-job`` needs ``--source-job-id``."""
    assert run("submit-volt", str(TEST_OBS_ID)).code == EXIT_USAGE
    assert run("submit-image-from-job", str(TEST_OBS_ID)).code == EXIT_USAGE
    assert run("submit-image-from-job", "--source-job-id", "0", str(TEST_OBS_ID)).code == EXIT_USAGE


def test_voltage_options_are_sent(run: Callable[..., Result], httpserver: HTTPServer, mock_login: None) -> None:
    """The short options of ``submit-volt`` set the offset, duration and channels."""
    serve_submission(httpserver, VOLTAGE_PATH)

    result = run("submit-volt", "-o", "10", "-u", "32", "-f", "3", "-t", "9", "-r", str(TEST_OBS_ID))

    assert result.code == EXIT_OK, result.err
    (body,) = bodies_sent_to(httpserver, VOLTAGE_PATH)
    assert body == gs.voltage_job_params(TEST_OBS_ID, 10, 32, from_channel=3, to_channel=9, allow_resubmit=True)


def test_a_failed_submission_does_not_stop_the_others(
    run: Callable[..., Result], httpserver: HTTPServer, mock_login: None
) -> None:
    """Every obsid is tried; the failure is reported, the others succeed, and the exit code is 1."""

    def handler(request: Request) -> Response:
        if json.loads(request.get_data())["obs_id"] == OTHER_OBS_ID:
            return Response(
                json.dumps(error_response("NOPE", "no thanks")), status=400, content_type="application/json"
            )
        return Response(
            json.dumps({"job_id": SUBMITTED_JOB_ID, "message": "ok", "status": "success"}),
            content_type="application/json",
        )

    httpserver.expect_request(DOWNLOAD_PATH, method="POST").respond_with_handler(handler)

    result = run("submit-vis", str(TEST_OBS_ID), str(OTHER_OBS_ID), str(THIRD_OBS_ID))

    assert result.code == EXIT_FAILED
    lines = log_lines(result)
    assert any(line.startswith(f"[ERROR] Obsid {OTHER_OBS_ID}:") and "NOPE" in line for line in lines)
    assert "[INFO] Submitted 2 of 3 obsids for visibility download." in lines
    assert f"Error: 1 of 3 obsids failed:\n  {OTHER_OBS_ID}:" in result.err
    assert len(bodies_sent_to(httpserver, DOWNLOAD_PATH)) == 3


def test_submit_with_wait_polls_until_the_job_is_ready(
    run: Callable[..., Result], httpserver: HTTPServer, mock_login: None, no_sleep: list[float]
) -> None:
    """``--wait`` polls the job list, logs each state change, and ends when the job is ready."""
    serve_submission(httpserver, DOWNLOAD_PATH)
    states = iter(["queued", "queued", "staging", "completed"])

    def jobs_handler(_: Request) -> Response:
        job = job_detail(SUBMITTED_JOB_ID, next(states))
        return Response(json.dumps({"jobs": [job], "total_count": 1}), content_type="application/json")

    httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_handler(jobs_handler)

    result = run("submit-vis", "--wait", str(TEST_OBS_ID))

    assert result.code == EXIT_OK, result.err
    lines = log_lines(result)
    assert "[INFO] Waiting for 1 jobs to be ready..." in lines
    assert lines.count(f"[INFO] Job ID {SUBMITTED_JOB_ID} (obsid: {TEST_OBS_ID}): is Queued") == 1
    assert "[INFO] All 1 MWA ASVO jobs are ready for download." in lines
    assert no_sleep[0] == cli_commands.WAIT_INITIAL_DELAY_S
    assert no_sleep.count(cli_commands.WAIT_POLL_INTERVAL_S) == 3


# wait and cancel


def test_wait_prints_the_jobs_when_they_are_ready(
    run: Callable[..., Result], three_jobs: None, no_sleep: list[float]
) -> None:
    """``wait`` ends at once for a ready job, then prints it as ``list`` does."""
    result = run("wait", "--json", str(LISTED_JOB_ID))

    assert result.code == EXIT_OK, result.err
    assert list(json.loads(result.out)) == [str(LISTED_JOB_ID)]
    assert no_sleep == [cli_commands.WAIT_INITIAL_DELAY_S]


def test_wait_fails_for_a_job_with_an_error(
    run: Callable[..., Result], three_jobs: None, no_sleep: list[float]
) -> None:
    """A job that failed ends the wait with an error."""
    result = run("wait", str(FAILED_JOB_ID))

    assert result.code == EXIT_FAILED
    assert f"has an error (code {FAILED_ERROR_CODE}): boom" in result.err


def test_wait_needs_a_job_id(run: Callable[..., Result]) -> None:
    """``wait`` and ``cancel`` with no job IDs say so."""
    assert "No jobids specified!" in run("wait").err
    assert "No jobids specified!" in run("cancel").err
    assert "No jobids specified!" in run("wait", str(TEST_OBS_ID)).err


def test_cancel_cancels_each_job_and_carries_on_after_a_failure(
    run: Callable[..., Result], httpserver: HTTPServer, mock_login: None
) -> None:
    """Each job is cancelled; a failure is logged and the count is of those that worked."""
    httpserver.expect_request(f"{JOBS_PATH}/{LISTED_JOB_ID}", method="DELETE").respond_with_json(
        {"job_id": LISTED_JOB_ID, "message": "Job cancelled", "status": "success"}
    )
    httpserver.expect_request(f"{JOBS_PATH}/{QUEUED_JOB_ID}", method="DELETE").respond_with_json(
        error_response("NOT_FOUND", "No such job"), status=404
    )

    result = run("cancel", str(LISTED_JOB_ID), str(QUEUED_JOB_ID))

    assert result.code == EXIT_OK, result.err
    lines = log_lines(result)
    assert f"[INFO] Cancelled MWA ASVO job ID {LISTED_JOB_ID} (Job cancelled)" in lines
    assert any(line.startswith(f"[ERROR] Failed to cancel MWA ASVO job ID {QUEUED_JOB_ID}:") for line in lines)
    assert lines[-1] == "[INFO] Cancelled 1 jobs."


def test_cancel_dry_run_sends_nothing(run: Callable[..., Result], httpserver: HTTPServer) -> None:
    """``cancel --dry-run`` reports the requests and makes none."""
    result = run("cancel", "-n", str(LISTED_JOB_ID), str(QUEUED_JOB_ID))

    assert log_lines(result) == [
        f"[INFO] [dry run] Would DELETE {JOBS_PATH}/{LISTED_JOB_ID}",
        f"[INFO] [dry run] Would DELETE {JOBS_PATH}/{QUEUED_JOB_ID}",
        "[INFO] [dry run] Would have cancelled 2 jobids. Nothing was sent.",
    ]
    assert httpserver.log == []


# download


@pytest.fixture
def served_tar(httpserver: HTTPServer, serve_jobs: Callable[..., None]) -> bytes:
    """Serve a ready job and its tar file.

    Args:
        httpserver: The pytest-httpserver server.
        serve_jobs: Serves the job list.

    Returns:
        The tar file's bytes.
    """
    data = tar_bytes()
    serve_jobs([ready_job(httpserver, len(data), hashlib.sha1(data).hexdigest())])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data(data)
    return data


def test_download_dry_run_counts_the_ids(run: Callable[..., Result], tmp_path: Path) -> None:
    """``download --dry-run`` says how many job IDs and obsids it parsed, and downloads nothing."""
    result = run("download", "-n", "-d", str(tmp_path), str(LISTED_JOB_ID), str(TEST_OBS_ID), "--keep-zip")

    assert result.code == EXIT_OK, result.err
    assert log_lines(result) == ["[INFO] Parsed 1 jobids and 1 obsids for download. keep_tar=true, hash=true"]
    assert list(tmp_path.glob("*.tar")) == []


def test_download_checks_its_arguments(run: Callable[..., Result], tmp_path: Path) -> None:
    """A missing directory and no job IDs are errors."""
    assert "No jobs or obsids specified!" in run("download").err
    result = run("download", "-d", str(tmp_path / "missing"), str(LISTED_JOB_ID))
    assert result.code == EXIT_FAILED
    assert "does not exist or is not accessible" in result.err


def test_download_unpacks_the_tar(
    run: Callable[..., Result], mock_login: None, served_tar: bytes, tmp_path: Path
) -> None:
    """A job ID is downloaded and unpacked into the directory."""
    target = tmp_path / "out"
    target.mkdir()

    result = run("download", "-d", str(target), str(JOB_ID))

    assert result.code == EXIT_OK, result.err
    assert (target / MEMBER_NAME).read_bytes() == MEMBER_CONTENTS
    assert "[INFO] Downloaded 1 of 1." in log_lines(result)


def test_download_by_obsid_with_keep_tar(
    run: Callable[..., Result], mock_login: None, served_tar: bytes, tmp_path: Path
) -> None:
    """An obsid finds its one ready job; ``--keep-tar`` leaves the tar file."""
    result = run("download", "-d", str(tmp_path), "-k", str(TEST_OBS_ID))

    assert result.code == EXIT_OK, result.err
    assert (tmp_path / f"{TEST_OBS_ID}_{JOB_ID}_vis.tar").read_bytes() == served_tar


def test_download_reports_a_hash_mismatch_and_fails(
    run: Callable[..., Result],
    mock_login: None,
    httpserver: HTTPServer,
    serve_jobs: Callable[..., None],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A wrong hash is an error, and the exit code is 1; with ``--skip-hash`` the same download works."""
    # A hash mismatch is retried for 15 minutes by default. The environment variable turns the retries off.
    monkeypatch.setenv("GIANT_SQUID_DOWNLOAD_RETRY_SECS", "0")
    data = tar_bytes()
    serve_jobs([ready_job(httpserver, len(data), hashlib.sha1(b"other").hexdigest())])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data(data)

    failed = run("download", "-d", str(tmp_path), "-k", str(JOB_ID))
    skipped = run("download", "-d", str(tmp_path), "-k", "-r", "--skip-hash", str(JOB_ID))

    assert failed.code == EXIT_FAILED
    assert "1 of 1 downloads failed; see the errors above." in failed.err
    assert "[INFO] Downloaded 0 of 1." in log_lines(failed)
    assert skipped.code == EXIT_OK, skipped.err


def test_download_runs_two_downloads_at_once(
    run: Callable[..., Result],
    mock_login: None,
    httpserver: HTTPServer,
    serve_jobs: Callable[..., None],
    tmp_path: Path,
) -> None:
    """Two jobs are downloaded in two threads, and each file is numbered in its label."""
    data = tar_bytes()
    sha1 = hashlib.sha1(data).hexdigest()
    first = ready_job(httpserver, len(data), sha1)
    second = ready_job(httpserver, len(data), sha1)
    second["id"] = SECOND_JOB_ID
    second["product"]["files"][0]["url"] = httpserver.url_for(SECOND_FILE_PATH)
    serve_jobs([first, second])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data(data)
    httpserver.expect_request(SECOND_FILE_PATH, method="GET").respond_with_data(data)

    result = run("download", "-c", "2", "-d", str(tmp_path), "-k", str(JOB_ID), str(SECOND_JOB_ID))

    assert result.code == EXIT_OK, result.err
    assert len(list(tmp_path.glob("*.tar"))) == 2
    assert "[INFO] Downloaded 2 of 2." in log_lines(result)
    assert any("[1/2]" in line for line in log_lines(result))
    assert any("[2/2]" in line for line in log_lines(result))


def test_the_download_buffer_size_comes_from_the_environment(
    run: Callable[..., Result], monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """A buffer size that is not a whole number of MiB is an error."""
    monkeypatch.setenv("GIANT_SQUID_BUF_SIZE", "lots")

    result = run("download", "-d", str(tmp_path), str(LISTED_JOB_ID))

    assert result.code == EXIT_FAILED
    assert "GIANT_SQUID_BUF_SIZE='lots' is not valid" in result.err


# environment and entry points


def test_a_missing_api_key_is_an_error(run: Callable[..., Result], monkeypatch: pytest.MonkeyPatch) -> None:
    """Without ``MWA_ASVO_API_KEY`` the program says so and makes no request."""
    monkeypatch.delenv("MWA_ASVO_API_KEY")

    result = run("list")

    assert result.code == EXIT_FAILED
    assert result.err.startswith("Error:")
    assert "key" in result.err.lower()


def test_the_session_is_cached_under_home(run: Callable[..., Result], three_jobs: None, cli_env: Path) -> None:
    """The session is kept in ``$HOME/.mwa-asvo/tokens.json``, the file that the Rust command uses."""
    run("list")

    assert (cli_env / ".mwa-asvo" / "tokens.json").is_file()


@pytest.mark.parametrize(
    "command",
    [
        "list", "download", "submit-vis", "submit-conv", "submit-image", "submit-image-from-job", "submit-meta",
        "submit-volt", "submit-bf", "wait", "cancel",
    ],
)  # fmt: skip
def test_every_command_has_help(run: Callable[..., Result], command: str) -> None:
    """``--help`` prints the usage of each command."""
    result = run(command, "--help")

    assert result.code == EXIT_OK
    assert f"giant-squid {command}" in result.out


def test_the_version_is_the_module_version(run: Callable[..., Result]) -> None:
    """``--version`` prints the version, as the Rust command does."""
    result = run("--version")

    assert result.out.strip() == f"mwa_giant_squid {gs.__version__}"


def test_the_module_runs_as_a_program(cli_env: Path) -> None:
    """``python -m mwa_giant_squid_cli`` is the command."""
    result = subprocess.run(
        [sys.executable, "-m", "mwa_giant_squid_cli", "submit-vis", "-n", str(TEST_OBS_ID)],
        capture_output=True,
        text=True,
        timeout=SUBPROCESS_TIMEOUT_S,
        check=False,
    )

    assert result.returncode == EXIT_OK, result.stderr
    assert "[INFO] [dry run] Would POST" in result.stderr


def test_ctrl_c_ends_concurrent_downloads(
    cli_env: Path, mock_login: None, httpserver: HTTPServer, serve_jobs: Callable[..., None], tmp_path: Path
) -> None:
    """Ctrl-C while two downloads run ends the program with exit code 130, and does not hang."""
    data = b"x" * (CHUNKS * CHUNK_SIZE)
    sha1 = hashlib.sha1(data).hexdigest()
    first = ready_job(httpserver, len(data), sha1)
    second = ready_job(httpserver, len(data), sha1)
    second["id"] = SECOND_JOB_ID
    second["product"]["files"][0]["url"] = httpserver.url_for(SECOND_FILE_PATH)
    serve_jobs([first, second])

    def slow(_: Request) -> Response:
        def stream() -> Any:
            for start in range(0, len(data), CHUNK_SIZE):
                time.sleep(CHUNK_DELAY_S)
                yield data[start : start + CHUNK_SIZE]

        return Response(stream(), headers={"Content-Length": str(len(data))})

    httpserver.expect_request(FILE_PATH, method="GET").respond_with_handler(slow)
    httpserver.expect_request(SECOND_FILE_PATH, method="GET").respond_with_handler(slow)

    # A process started in the background can have SIGINT ignored. The program needs Python's handler, which
    # raises KeyboardInterrupt, so the child sets it.
    start = (
        "import signal, sys; signal.signal(signal.SIGINT, signal.default_int_handler); "
        "from mwa_giant_squid_cli import main; sys.exit(main(sys.argv[1:]))"
    )
    process = subprocess.Popen(
        [
            sys.executable,
            "-c",
            start,
            "download",
            "-c",
            "2",
            "-k",
            "-d",
            str(tmp_path),
            str(JOB_ID),
            str(SECOND_JOB_ID),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    time.sleep(SIGINT_DELAY_S)
    process.send_signal(signal.SIGINT)
    try:
        _, err = process.communicate(timeout=MAX_STOP_TIME_S)
    except subprocess.TimeoutExpired:
        process.kill()
        pytest.fail("the program did not stop after Ctrl-C")

    assert process.returncode == EXIT_INTERRUPTED, err
    assert "Interrupted." in err
