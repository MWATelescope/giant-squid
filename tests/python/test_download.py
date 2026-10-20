"""Tests for AsvoClient.download_job and download_obs, against a local mock MWA ASVO.

The mock serves the job's file as well as the API, so a download runs end to end: the bytes are fetched, written
or unpacked, and the SHA-1 is checked. Nothing is fetched from Acacia. The Ctrl-C tests send SIGINT to this
process from a timer thread while a slow download runs on the main thread.
"""

import datetime
import hashlib
import io
import logging
import os
import pathlib
import signal
import tarfile
import threading
import time
from collections.abc import Callable, Iterator
from typing import Any

import pytest
from pytest_httpserver import HTTPServer
from werkzeug import Request, Response

import mwa_giant_squid as gs

from .conftest import GET_JOBS_PATH, TEST_API_KEY, TEST_OBS_ID, job_detail

# The job that the mock says is ready, and the name its file is served and saved under.
JOB_ID = 12345
FILE_NAME = f"{TEST_OBS_ID}_{JOB_ID}_vis.tar"
FILE_PATH = f"/downloads/{FILE_NAME}"

# The file inside the tar.
MEMBER_NAME = "1065880128_metafits.fits"
MEMBER_CONTENTS = b"not really a metafits file\n" * 200

# A slow download: CHUNKS chunks of CHUNK_SIZE bytes, one every CHUNK_DELAY seconds.
CHUNKS = 60
CHUNK_SIZE = 1024
CHUNK_DELAY = 0.05

# When the timer thread sends SIGINT, and the longest a stop may take after it.
SIGINT_AFTER = 0.3
MAX_STOP_TIME = 2.0

# How long the log handler takes to write the ERROR record of a failed attempt, in the test of a Ctrl-C that lands in
# a log call. It is longer than SIGINT_AFTER, so that the signal arrives while that log call is running.
SLOW_LOG_DELAY = 0.4
# A retry duration that is short, so that a download that did not stop ends the test in seconds, not minutes.
SHORT_RETRY = 10.0

# A retry duration long enough that, without a stop, a test would wait for many back-off intervals.
LONG_RETRY = 300.0


def tar_bytes() -> bytes:
    """A tar archive that holds one file.

    Returns:
        The archive.
    """
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as tar:
        info = tarfile.TarInfo(MEMBER_NAME)
        info.size = len(MEMBER_CONTENTS)
        tar.addfile(info, io.BytesIO(MEMBER_CONTENTS))
    return buffer.getvalue()


def ready_job(httpserver: HTTPServer, size: int, sha1: str) -> dict[str, Any]:
    """A completed job whose file is served by the mock.

    Args:
        httpserver: The pytest-httpserver server.
        size: The file size the job gives.
        sha1: The SHA-1 the job gives.

    Returns:
        The job, as the server sends it.
    """
    product = {"files": [{"type": "acacia", "url": httpserver.url_for(FILE_PATH), "size": size, "sha1": sha1}]}
    return job_detail(JOB_ID, "completed", product=product)


@pytest.fixture
def client(host: str, mock_login: None) -> gs.AsvoClient:
    """A client that has logged in to the mock.

    Args:
        host: The mock's base URL.
        mock_login: Serves the login.

    Returns:
        The client.
    """
    return gs.AsvoClient(host, TEST_API_KEY)


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


@pytest.fixture
def slow_file(httpserver: HTTPServer, serve_jobs: Callable[..., None]) -> None:
    """Serve a ready job whose file arrives slowly, in chunks.

    Args:
        httpserver: The pytest-httpserver server.
        serve_jobs: Serves the job list.
    """
    data = b"x" * (CHUNKS * CHUNK_SIZE)
    serve_jobs([ready_job(httpserver, len(data), hashlib.sha1(data).hexdigest())])

    def stream() -> Iterator[bytes]:
        for start in range(0, len(data), CHUNK_SIZE):
            time.sleep(CHUNK_DELAY)
            yield data[start : start + CHUNK_SIZE]

    def handler(_: Request) -> Response:
        return Response(stream(), headers={"Content-Length": str(len(data))})

    httpserver.expect_request(FILE_PATH, method="GET").respond_with_handler(handler)


@pytest.fixture
def sigint_raises() -> Iterator[None]:
    """Make SIGINT raise KeyboardInterrupt in this process, for the test.

    Python installs that handler at start-up only if SIGINT is not ignored. A process started in the
    background by a non-interactive shell (for example ``cmd &`` in a script, or some CI runners) starts
    with SIGINT ignored, and the Ctrl-C tests would then wait for a signal that never arrives.

    Yields:
        Nothing; the old handler is put back afterwards.
    """
    previous = signal.signal(signal.SIGINT, signal.default_int_handler)
    try:
        yield
    finally:
        signal.signal(signal.SIGINT, previous)


class SlowLogHandler(logging.Handler):
    """A log handler that takes a while to write a record, like one that fails and prints a traceback.

    It handles ERROR records only, which is the one that the download logs for each failed attempt.
    """

    def __init__(self) -> None:
        """Make the handler."""
        super().__init__(level=logging.ERROR)

    def emit(self, record: logging.LogRecord) -> None:
        """Take SLOW_LOG_DELAY seconds, and write nothing.

        Args:
            record: The record.
        """
        time.sleep(SLOW_LOG_DELAY)


@pytest.fixture
def slow_logging() -> Iterator[None]:
    """Log the module's records through a slow handler, for the test.

    The Rust code logs through Python's ``logging``, and Python runs a pending signal handler at the next bytecode,
    which can be inside such a log call.

    Yields:
        Nothing; the logging set-up is put back afterwards.
    """
    root = logging.getLogger()
    handler = SlowLogHandler()
    old_level = root.level
    root.addHandler(handler)
    root.setLevel(logging.DEBUG)
    gs.reset_logging()
    try:
        yield
    finally:
        root.removeHandler(handler)
        root.setLevel(old_level)
        gs.reset_logging()


def send_sigint_soon() -> threading.Timer:
    """Send SIGINT to this process after SIGINT_AFTER seconds, from another thread.

    Returns:
        The timer, already started.
    """
    timer = threading.Timer(SIGINT_AFTER, os.kill, (os.getpid(), signal.SIGINT))
    timer.start()
    return timer


def test_download_job_id_keeps_the_tar_and_checks_the_hash(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """With keep_tar, the tar file is saved as it is."""
    client.download_job(JOB_ID, tmp_path, keep_tar=True, job_subdirectory=False)

    assert (tmp_path / FILE_NAME).read_bytes() == served_tar


def test_download_job_id_unpacks_the_tar_by_default(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """Without keep_tar, the tar is unpacked into the directory while it downloads."""
    client.download_job(JOB_ID, tmp_path, job_subdirectory=False)

    assert (tmp_path / MEMBER_NAME).read_bytes() == MEMBER_CONTENTS
    assert not (tmp_path / FILE_NAME).exists()


def test_a_download_returns_the_job_it_downloaded(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """download_job and download_obs return the job, so a caller learns the job ID of an obsid."""
    by_obs_id = client.download_obs(TEST_OBS_ID, str(tmp_path), keep_tar=True)

    assert by_obs_id.job_id == JOB_ID
    assert by_obs_id.obs_id == TEST_OBS_ID
    assert len(served_tar) > 0


def test_a_download_from_a_job_list_gets_no_job_list(
    client: gs.AsvoClient, served_tar: bytes, httpserver: HTTPServer, tmp_path: pathlib.Path
) -> None:
    """With ``jobs``, download_job and download_obs find the job there and make no job list request."""
    jobs = client.get_jobs()
    listings = sum(1 for request, _ in httpserver.log if request.path == GET_JOBS_PATH)

    client.download_job(JOB_ID, tmp_path, keep_tar=True, jobs=jobs, job_subdirectory=False)
    (tmp_path / FILE_NAME).unlink()
    client.download_obs(TEST_OBS_ID, tmp_path, keep_tar=True, jobs=jobs, job_subdirectory=False)

    assert sum(1 for request, _ in httpserver.log if request.path == GET_JOBS_PATH) == listings
    assert (tmp_path / FILE_NAME).read_bytes() == served_tar


def test_download_obs_id_finds_the_ready_job(client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path) -> None:
    """download_obs downloads the one ready job of the obsid."""
    client.download_obs(TEST_OBS_ID, str(tmp_path), keep_tar=True, job_subdirectory=False)

    assert (tmp_path / FILE_NAME).read_bytes() == served_tar


def test_progress_reports_every_byte_between_started_and_finished(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """The events are Started, then Advanced, then Finished, and the Advanced bytes add up to the file size."""
    events: list[gs.DownloadProgress] = []

    client.download_job(JOB_ID, tmp_path, keep_tar=True, progress=events.append)

    first, *middle, last = events
    assert isinstance(first, gs.DownloadProgress.Started)
    assert first.job_id == JOB_ID
    assert first.total_bytes == len(served_tar)
    assert first.position == 0
    assert "[1/1]" in first.label
    assert isinstance(last, gs.DownloadProgress.Finished)
    assert all(isinstance(event, gs.DownloadProgress.Advanced) for event in middle)
    assert sum(event.bytes for event in middle if isinstance(event, gs.DownloadProgress.Advanced)) == len(served_tar)


def test_progress_combines_the_advanced_events(client: gs.AsvoClient, slow_file: None, tmp_path: pathlib.Path) -> None:
    """A slow download of many chunks gives far fewer Advanced events than chunks, with every byte counted."""
    events: list[gs.DownloadProgress] = []

    client.download_job(JOB_ID, tmp_path, keep_tar=True, progress=events.append)

    advanced = [event for event in events if isinstance(event, gs.DownloadProgress.Advanced)]
    assert 0 < len(advanced) < CHUNKS
    assert sum(event.bytes for event in advanced) == CHUNKS * CHUNK_SIZE


def test_the_label_numbers_the_download_in_a_series(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """download_number and download_count go into the label."""
    events: list[gs.DownloadProgress] = []

    client.download_job(JOB_ID, tmp_path, keep_tar=True, progress=events.append, download_number=2, download_count=3)

    started = events[0]
    assert isinstance(started, gs.DownloadProgress.Started)
    assert "[2/3]" in started.label


def test_an_exception_in_the_callback_stops_the_download_and_is_raised(
    client: gs.AsvoClient, slow_file: None, tmp_path: pathlib.Path
) -> None:
    """The callback's own exception reaches the caller, and the download does not run to the end."""

    class StopHere(Exception):
        """Raised by the callback."""

    def progress(event: gs.DownloadProgress) -> None:
        if isinstance(event, gs.DownloadProgress.Started):
            raise StopHere

    started = time.monotonic()
    with pytest.raises(StopHere):
        client.download_job(JOB_ID, tmp_path, keep_tar=True, progress=progress)

    assert time.monotonic() - started < CHUNKS * CHUNK_DELAY


@pytest.mark.parametrize("progress", [None, lambda _: None], ids=["no callback", "callback"])
@pytest.mark.usefixtures("sigint_raises")
def test_ctrl_c_stops_a_download(
    client: gs.AsvoClient, slow_file: None, tmp_path: pathlib.Path, progress: Callable[..., None] | None
) -> None:
    """SIGINT during a download raises KeyboardInterrupt soon, with or without a callback."""
    timer = send_sigint_soon()
    started = time.monotonic()
    try:
        with pytest.raises(KeyboardInterrupt):
            client.download_job(JOB_ID, tmp_path, keep_tar=True, progress=progress, job_subdirectory=False)
    finally:
        timer.cancel()

    assert time.monotonic() - started < SIGINT_AFTER + MAX_STOP_TIME
    # The partial file stays, so a later download can resume it.
    assert (tmp_path / FILE_NAME).stat().st_size < CHUNKS * CHUNK_SIZE


@pytest.mark.usefixtures("sigint_raises")
def test_ctrl_c_stops_the_wait_before_a_retry(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """A download that is waiting to retry a failure stops at once on SIGINT, not after the back-off."""
    serve_jobs([ready_job(httpserver, 1, hashlib.sha1(b"x").hexdigest())])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data("transient fault", status=500)

    timer = send_sigint_soon()
    started = time.monotonic()
    try:
        with pytest.raises(KeyboardInterrupt):
            client.download_job(JOB_ID, tmp_path, keep_tar=True, retry_duration=LONG_RETRY)
    finally:
        timer.cancel()

    assert time.monotonic() - started < SIGINT_AFTER + MAX_STOP_TIME


@pytest.mark.usefixtures("sigint_raises", "slow_logging")
def test_ctrl_c_that_lands_in_a_log_call_still_stops_the_download(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """A KeyboardInterrupt raised inside a log call is not lost: the download stops and raises it.

    Python runs the SIGINT handler at the next bytecode, and that can be in the logging call that the Rust code makes
    for a failed attempt. The exception then leaves the log call, and the library, which cannot return an error from
    a log call, leaves it as the current exception. Before the fix nothing looked at it, the signal was spent, and the
    download kept retrying until its retry duration ran out.
    """
    serve_jobs([ready_job(httpserver, 1, hashlib.sha1(b"x").hexdigest())])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data("transient fault", status=500)

    timer = send_sigint_soon()
    started = time.monotonic()
    try:
        with pytest.raises(KeyboardInterrupt):
            client.download_job(JOB_ID, tmp_path, keep_tar=True, retry_duration=SHORT_RETRY)
    finally:
        timer.cancel()

    # The signal ends the slow log call that it lands in, so the wait does not have to be long.
    assert time.monotonic() - started < SIGINT_AFTER + SLOW_LOG_DELAY + MAX_STOP_TIME


def test_a_buffer_size_of_zero_raises_value_error_before_any_request(
    client: gs.AsvoClient, tmp_path: pathlib.Path
) -> None:
    """A buffer of 0 bytes would write nothing, so the library refuses it (the server has no job list)."""
    with pytest.raises(ValueError, match="buffer_size"):
        client.download_job(JOB_ID, tmp_path, buffer_size=0)
    with pytest.raises(ValueError, match="buffer_size"):
        client.download_obs(TEST_OBS_ID, tmp_path, buffer_size=0)


def test_a_hash_mismatch_raises_asvo_error(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """A file whose SHA-1 is not the job's raises AsvoError with kind HashMismatch."""
    data = tar_bytes()
    serve_jobs([ready_job(httpserver, len(data), "0" * 40)])
    httpserver.expect_request(FILE_PATH, method="GET").respond_with_data(data)

    with pytest.raises(gs.AsvoError) as err:
        client.download_job(JOB_ID, tmp_path, keep_tar=True, retry_duration=0)

    assert err.value.kind == "HashMismatch"
    assert err.value.job_id == JOB_ID


def test_a_dug_job_raises_files_not_reachable(
    client: gs.AsvoClient, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """A job delivered to DUG cannot be downloaded from here: it raises AsvoError with kind FilesNotReachable."""
    serve_jobs(
        [job_detail(JOB_ID, "completed", product={"files": [{"type": "dug", "path": "/dug/asvo/12345", "size": 5}]})]
    )

    with pytest.raises(gs.AsvoError) as err:
        client.download_job(JOB_ID, tmp_path)

    assert err.value.kind == "FilesNotReachable"
    assert err.value.job_id == JOB_ID
    assert err.value.delivery == gs.Type.Dug
    assert err.value.path == "/dug/asvo/12345"


def test_an_unknown_job_raises_asvo_error(
    client: gs.AsvoClient, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """A job ID that is not in the job list raises AsvoError with kind NoAsvoJob."""
    serve_jobs([])

    with pytest.raises(gs.AsvoError) as err:
        client.download_job(JOB_ID, tmp_path)

    assert err.value.kind == "NoAsvoJob"


def test_an_obs_id_with_no_ready_job_raises_asvo_error(
    client: gs.AsvoClient, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """An obsid whose only job is still queued raises AsvoError with kind NoJobReadyForObsId."""
    serve_jobs([job_detail(JOB_ID, "queued")])

    with pytest.raises(gs.AsvoError) as err:
        client.download_obs(TEST_OBS_ID, tmp_path)

    assert err.value.kind == "NoJobReadyForObsId"
    assert err.value.obs_id == TEST_OBS_ID


def test_an_invalid_obs_id_is_rejected_before_any_request(
    client: gs.AsvoClient, httpserver: HTTPServer, tmp_path: pathlib.Path
) -> None:
    """download_obs checks the obsid first."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="Obs ID"):
        client.download_obs(1, tmp_path)

    assert len(httpserver.log) == requests_before


def test_a_bad_retry_duration_is_rejected(client: gs.AsvoClient, tmp_path: pathlib.Path) -> None:
    """A negative retry_duration raises ValueError."""
    with pytest.raises(ValueError, match="retry_duration"):
        client.download_job(JOB_ID, tmp_path, retry_duration=-1.0)


def test_the_progress_events_have_readable_reprs() -> None:
    """repr() shows the variant and its fields."""
    assert repr(gs.DownloadProgress.Advanced(bytes=5)) == "DownloadProgress.Advanced(bytes=5)"
    assert repr(gs.DownloadProgress.Finished()) == "DownloadProgress.Finished()"


def test_a_job_subdirectory_puts_the_files_in_a_directory_named_for_the_job(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """With job_subdirectory, the files go in download_dir/<job_id>, which is made."""
    client.download_job(JOB_ID, tmp_path, job_subdirectory=True)

    assert (tmp_path / str(JOB_ID) / MEMBER_NAME).read_bytes() == MEMBER_CONTENTS
    assert [entry.name for entry in tmp_path.iterdir()] == [str(JOB_ID)]


def test_a_job_subdirectory_holds_a_kept_tar_too(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """The tar that keep_tar keeps is in the directory of the job."""
    client.download_job(JOB_ID, tmp_path, keep_tar=True, job_subdirectory=True)

    assert (tmp_path / str(JOB_ID) / FILE_NAME).read_bytes() == served_tar
    assert not (tmp_path / FILE_NAME).exists()


def test_without_a_job_subdirectory_the_files_stay_in_the_download_directory(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """With job_subdirectory off, the layout is that of 2.x: no directory for the job."""
    client.download_job(JOB_ID, tmp_path, job_subdirectory=False)

    assert (tmp_path / MEMBER_NAME).read_bytes() == MEMBER_CONTENTS
    assert not (tmp_path / str(JOB_ID)).exists()


def test_the_files_go_in_a_directory_for_the_job_by_default(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """Without job_subdirectory, the files go in download_dir/<job_id>, as for the giant-squid command."""
    client.download_job(JOB_ID, tmp_path)

    assert (tmp_path / str(JOB_ID) / MEMBER_NAME).read_bytes() == MEMBER_CONTENTS
    assert [entry.name for entry in tmp_path.iterdir()] == [str(JOB_ID)]


def test_download_obs_uses_the_directory_of_the_job_that_it_finds(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """An Obs ID is downloaded into the directory of its one ready job."""
    client.download_obs(int(TEST_OBS_ID), tmp_path, job_subdirectory=True)

    assert (tmp_path / str(JOB_ID) / MEMBER_NAME).is_file()


def test_a_job_subdirectory_that_is_a_symlink_is_refused(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """A link where the directory of the job should be is not followed."""
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    download_dir = tmp_path / "downloads"
    download_dir.mkdir()
    (download_dir / str(JOB_ID)).symlink_to(elsewhere)

    with pytest.raises(gs.AsvoError) as info:
        client.download_job(JOB_ID, download_dir, job_subdirectory=True)

    assert info.value.kind == "SymlinkInDownloadDir"
    assert list(elsewhere.iterdir()) == []


def test_a_download_directory_that_is_missing_is_a_file_not_found_error(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """A file system error is an OSError, as Python's open raises it, and not an AsvoError."""
    with pytest.raises(FileNotFoundError, match="download directory") as info:
        client.download_job(JOB_ID, tmp_path / "missing", job_subdirectory=True)

    assert not isinstance(info.value, gs.AsvoError)


def test_a_job_directory_that_cannot_be_made_is_an_os_error_with_its_path(
    client: gs.AsvoClient, served_tar: bytes, tmp_path: pathlib.Path
) -> None:
    """A file where the directory of the job should be is an OSError, and it names the path."""
    (tmp_path / str(JOB_ID)).write_text("not a directory")

    with pytest.raises(OSError) as info:
        client.download_job(JOB_ID, tmp_path, job_subdirectory=True)

    assert not isinstance(info.value, gs.AsvoError)
    assert info.value.filename is not None
    assert str(JOB_ID) in str(info.value.filename)


# The `Expires` of a signed URL that ended long ago (2001-09-09T01:46:40Z), and of one that has not ended (2100).
EXPIRES_LONG_AGO = 1_000_000_000
EXPIRES_FAR_AWAY = 4_102_444_800


def job_with_url(httpserver: HTTPServer, job_id: int, expires: int) -> dict[str, Any]:
    """A completed job whose Acacia URL has an `Expires`.

    Args:
        httpserver: The pytest-httpserver server.
        job_id: The Job ID.
        expires: The `Expires` of the URL, in seconds since the Unix epoch.

    Returns:
        The job, as the server sends it.
    """
    url = f"{httpserver.url_for(FILE_PATH)}?Expires={expires}"
    product = {"files": [{"type": "acacia", "url": url, "size": 7, "sha1": "0" * 40}]}
    return job_detail(job_id, "completed", product=product)


def test_a_job_says_when_its_download_links_expire(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """`expires`, `is_expired` and `display_state` follow the `Expires` of the URL, and `job_state` stays the API's."""
    serve_jobs([job_with_url(httpserver, 101, EXPIRES_FAR_AWAY), job_with_url(httpserver, 102, EXPIRES_LONG_AGO)])

    ready, expired = sorted(client.get_jobs(), key=lambda job: job.job_id)

    assert (ready.is_expired, ready.display_state) == (False, "completed (ready)")
    assert (expired.is_expired, expired.display_state) == (True, "completed (expired)")
    assert expired.expires == datetime.datetime(2001, 9, 9, 1, 46, 40, tzinfo=datetime.timezone.utc)
    assert ready.job_state == expired.job_state == gs.JobState.Completed


def test_a_download_of_a_job_whose_links_expired_raises_job_expired(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None], tmp_path: pathlib.Path
) -> None:
    """The error has the Job ID and the time, and no request is made for the file."""
    serve_jobs([job_with_url(httpserver, JOB_ID, EXPIRES_LONG_AGO)])

    with pytest.raises(gs.AsvoError) as info:
        client.download_job(JOB_ID, tmp_path)

    assert info.value.kind == "JobExpired"
    assert info.value.job_id == JOB_ID
    assert info.value.expired == datetime.datetime(2001, 9, 9, 1, 46, 40, tzinfo=datetime.timezone.utc)
    assert "Resubmit the job" in str(info.value)
    assert not any(request.path == FILE_PATH for request, _ in httpserver.log)


def test_all_ready_raises_job_expired_for_a_job_whose_links_expired(
    client: gs.AsvoClient, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """Waiting does not help an expired job, so `all_ready` fails as it does for a cancelled one."""
    serve_jobs([job_with_url(httpserver, 101, EXPIRES_FAR_AWAY), job_with_url(httpserver, 102, EXPIRES_LONG_AGO)])
    jobs = client.get_jobs()

    assert jobs.all_ready([101]) is True
    with pytest.raises(gs.AsvoError) as info:
        jobs.all_ready([101, 102])

    assert info.value.kind == "JobExpired"
    assert info.value.job_id == 102
