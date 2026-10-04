"""Tests for the ``giant-squid`` command: the Rust ``giant-squid`` program run inside the module.

Each test starts the command as a new process, because the Rust code writes to the real standard output and standard
error of the process, and because the command ends the process. The server is the local mock: no test reaches a real
MWA ASVO server.
"""

import importlib.metadata
import os
import signal
import subprocess
import sys
import sysconfig
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import GET_JOBS_PATH, TEST_API_KEY, TEST_OBS_ID, job_detail

EXIT_OK = 0
EXIT_FAILED = 1
EXIT_USAGE = 2

# The module that starts the command.
LAUNCHER_MODULE = "mwa_giant_squid_cli"

# The name of the installed distribution, and of its command.
DISTRIBUTION_NAME = "mwa-giant-squid"
COMMAND_NAME = "giant-squid"

# How long a command may run in these tests, in seconds.
RUN_TIMEOUT_S = 60

# The job that the `wait` tests wait for, which the mock never finishes.
WAITED_JOB_ID = 31

# How long to wait for the mock to get a request, in seconds, and how often to look.
REQUEST_WAIT_S = 30
REQUEST_POLL_S = 0.05


class Result:
    """What a run of the command wrote and returned."""

    def __init__(self, process: "subprocess.CompletedProcess[str]") -> None:
        """Keep the result of the process.

        Args:
            process: The finished process.
        """
        self.code = process.returncode
        self.out = process.stdout
        self.err = process.stderr


@pytest.fixture
def child_env(host: str, tmp_path: Path) -> dict[str, str]:
    """The environment of the command: the mock server, and an empty home directory.

    Args:
        host: The mock's base URL.
        tmp_path: A directory for the home directory.

    Returns:
        The environment variables.
    """
    env = {name: value for name, value in os.environ.items() if not name.startswith(("MWA_ASVO_", "GIANT_SQUID_"))}
    env.update({"HOME": str(tmp_path), "MWA_ASVO_HOST": host, "MWA_ASVO_API_KEY": TEST_API_KEY})
    return env


@pytest.fixture
def run(child_env: dict[str, str]) -> Callable[..., Result]:
    """A function that runs the command with arguments, and waits for it.

    Args:
        child_env: The environment.

    Returns:
        The function.
    """

    def run_native(*args: str) -> Result:
        process = subprocess.run(
            [sys.executable, "-m", LAUNCHER_MODULE, *args],
            env=child_env,
            capture_output=True,
            text=True,
            timeout=RUN_TIMEOUT_S,
            check=False,
        )
        return Result(process)

    return run_native


def test_the_installed_command_is_the_launcher_and_nothing_else() -> None:
    """The distribution has one command, ``giant-squid``, which starts the launcher."""
    entry_points = importlib.metadata.distribution(DISTRIBUTION_NAME).entry_points

    assert {ep.name: ep.value for ep in entry_points if ep.group == "console_scripts"} == {
        COMMAND_NAME: "mwa_giant_squid_cli:main"
    }


def test_the_installed_script_runs_the_rust_program(child_env: dict[str, str]) -> None:
    """The ``giant-squid`` script that the install made is the Rust program: clap's help, not argparse's."""
    suffix = ".exe" if sys.platform == "win32" else ""
    script = os.path.join(sysconfig.get_path("scripts"), COMMAND_NAME + suffix)

    process = subprocess.run(
        [script, "list", "--help"], env=child_env, capture_output=True, text=True, timeout=RUN_TIMEOUT_S, check=False
    )

    assert process.returncode == EXIT_OK, process.stderr
    assert "Usage: giant-squid list" in process.stdout


def test_help_is_the_rust_help_and_exits_zero(run: Callable[..., Result]) -> None:
    """``--help`` prints clap's help, under the name ``giant-squid``, to standard output."""
    result = run("--help")

    assert result.code == EXIT_OK
    assert "Usage: giant-squid" in result.out
    assert "submit-image-from-job" in result.out


def test_the_version_is_the_modules(run: Callable[..., Result]) -> None:
    """``--version`` prints the version of the crate, which the module has too.

    The name before the version is the name of the program (``giant-squid``), not the name of the crate.
    """
    result = run("--version")

    assert result.code == EXIT_OK
    assert result.out.split() == [COMMAND_NAME, gs.__version__]


def test_a_bad_argument_is_a_usage_error(run: Callable[..., Result]) -> None:
    """An option that does not exist is clap's usage error: code 2, and the text on standard error."""
    result = run("list", "--no-such-option")

    assert result.code == EXIT_USAGE
    assert "--no-such-option" in result.err
    assert result.out == ""


def test_an_error_of_the_command_is_code_one_with_the_message(run: Callable[..., Result]) -> None:
    """An obsid given to ``wait`` is refused before any request, as the Rust program refuses it."""
    result = run("wait", str(TEST_OBS_ID))

    assert result.code == EXIT_FAILED
    assert f"Error: Expected only job IDs, but found these obsids: {TEST_OBS_ID}." in result.err


def test_a_dry_run_logs_with_the_commands_own_logger(run: Callable[..., Result]) -> None:
    """The command's log lines come from its own logger (time stamp, level), not from Python's ``logging``.

    That is only so if the module has not connected the Rust log records to Python's ``logging`` when it is imported:
    there is one Rust logger per process, and the first one wins.
    """
    result = run("submit-vis", "--dry-run", str(TEST_OBS_ID))

    assert result.code == EXIT_OK, result.err
    assert "[dry run] Would POST /api/v2/download_vis_job" in result.err
    assert "INFO" in result.err
    assert result.out == ""


@pytest.mark.usefixtures("mock_login")
def test_list_reads_the_job_list_from_the_server(
    run: Callable[..., Result], serve_jobs: Callable[[list[dict[str, Any]]], None]
) -> None:
    """``list --json`` logs in to the mock, gets the jobs, and prints them to standard output."""
    serve_jobs([job_detail(WAITED_JOB_ID, "queued")])

    result = run("list", "--json")

    assert result.code == EXIT_OK, result.err
    assert f'"{WAITED_JOB_ID}"' in result.out


@pytest.mark.skipif(sys.platform == "win32", reason="the test sends SIGINT and reads the signal from the exit status")
@pytest.mark.usefixtures("mock_login")
def test_ctrl_c_ends_the_process_at_once_like_the_rust_program(
    httpserver: HTTPServer,
    child_env: dict[str, str],
    serve_jobs: Callable[[list[dict[str, Any]]], None],
) -> None:
    """``wait`` on a job that never finishes is ended by SIGINT: the process dies of the signal, with no traceback.

    A process that runs in the background can start with SIGINT ignored. The launcher puts the default action back,
    so the test does not have to.
    """
    serve_jobs([job_detail(WAITED_JOB_ID, "queued")])
    process = subprocess.Popen(
        [sys.executable, "-m", LAUNCHER_MODULE, "wait", str(WAITED_JOB_ID)],
        env=child_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        deadline = time.monotonic() + REQUEST_WAIT_S
        while not any(request.path == GET_JOBS_PATH for request, _ in httpserver.log):
            assert time.monotonic() < deadline, "the command did not ask for the jobs"
            time.sleep(REQUEST_POLL_S)
        process.send_signal(signal.SIGINT)
        _, err = process.communicate(timeout=RUN_TIMEOUT_S)
    except subprocess.TimeoutExpired:
        process.kill()
        pytest.fail("the command did not stop after Ctrl-C")

    assert process.returncode == -signal.SIGINT, err
    assert "Traceback" not in err
