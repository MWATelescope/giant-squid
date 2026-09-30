"""Tests for AsvoClient and the job types, against a local mock MWA ASVO."""

import datetime
import json
import logging
import pathlib
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import (
    GET_JOBS_PATH,
    LOGIN_PATH,
    TEST_API_KEY,
    TEST_OBS_ID,
    error_response,
    job_detail,
)

# Job IDs used by these tests.
JOB_ID_READY = 101
JOB_ID_QUEUED = 102
JOB_ID_FAILED = 103

# A completion time the mock sends (naive, as the real server sends it) and what it means (UTC).
COMPLETED_TEXT = "2026-09-08T06:00:00"
COMPLETED_UTC = datetime.datetime(2026, 9, 8, 6, 0, tzinfo=datetime.timezone.utc)

# A file in a completed job's product.
FILE_URL = "https://example.org/1065880128_101_vis.tar"
FILE_SIZE = 1234
FILE_SHA1 = "0000000000000000000000000000000000000000"

# How many threads share one client in the thread test.
THREADS = 4

# How many clients the no-cache test makes (each one logs in).
CLIENTS = 2

# The Python logger that is the parent of all the module's log records.
MODULE_LOGGER = "mwa_giant_squid"


def logins(httpserver: HTTPServer) -> int:
    """The number of login requests the mock has received.

    Args:
        httpserver: The pytest-httpserver server.

    Returns:
        The count.
    """
    return sum(1 for request, _ in httpserver.log if request.path == LOGIN_PATH)


def mixed_jobs() -> list[dict[str, Any]]:
    """A ready job with a file, a queued job and a failed job.

    Returns:
        The jobs, as the server sends them.
    """
    product = {"files": [{"type": "acacia", "url": FILE_URL, "size": FILE_SIZE, "sha1": FILE_SHA1}]}
    return [
        job_detail(JOB_ID_READY, "completed", completed=COMPLETED_TEXT, product=product),
        job_detail(JOB_ID_QUEUED, "queued"),
        job_detail(JOB_ID_FAILED, "error", error_text="the conversion failed"),
    ]


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_returns_the_jobs_with_their_fields(host: str, serve_jobs: Callable[..., None]) -> None:
    """Every field of a job comes through from the server."""
    serve_jobs(mixed_jobs())
    client = gs.AsvoClient(host, TEST_API_KEY)

    jobs = client.get_jobs()

    assert len(jobs) == len(mixed_jobs())
    ready = jobs[0]
    assert ready.job_id == JOB_ID_READY
    assert ready.obs_id == TEST_OBS_ID
    assert ready.job_type == gs.AsvoJobType.DownloadVisibilities
    assert ready.job_state == gs.AsvoJobState.Ready
    assert ready.error_text is None
    assert ready.completed == COMPLETED_UTC
    assert ready.files is not None
    (file,) = ready.files
    assert file.type == gs.Delivery.Acacia
    assert file.url == FILE_URL
    assert file.size == FILE_SIZE
    assert file.sha1 == FILE_SHA1
    assert file.path is None

    assert jobs[1].job_state == gs.AsvoJobState.Queued
    assert jobs[1].files is None
    assert jobs[-1].job_state == gs.AsvoJobState.Error
    assert jobs[-1].error_text == "the conversion failed"


@pytest.mark.usefixtures("mock_login")
def test_a_job_list_supports_iteration_and_rejects_a_bad_index(host: str, serve_jobs: Callable[..., None]) -> None:
    """AsvoJobVec behaves like a read-only sequence."""
    serve_jobs(mixed_jobs())
    jobs = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert [job.job_id for job in jobs] == [JOB_ID_READY, JOB_ID_QUEUED, JOB_ID_FAILED]
    with pytest.raises(IndexError):
        jobs[len(mixed_jobs())]


@pytest.mark.usefixtures("mock_login")
def test_filter_keeps_the_matching_jobs(host: str, serve_jobs: Callable[..., None]) -> None:
    """Each filter works, and states compare by kind."""
    serve_jobs(mixed_jobs())
    jobs = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert len(jobs.filter()) == len(mixed_jobs())
    assert [j.job_id for j in jobs.filter(job_ids=[JOB_ID_QUEUED])] == [JOB_ID_QUEUED]
    assert len(jobs.filter(obs_ids=[TEST_OBS_ID])) == len(mixed_jobs())
    assert len(jobs.filter(job_types=[gs.AsvoJobType.Conversion])) == 0
    errors_and_ready = jobs.filter(job_states=[gs.AsvoJobState.Error, gs.AsvoJobState.Ready])
    assert [j.job_id for j in errors_and_ready] == [JOB_ID_READY, JOB_ID_FAILED]
    with pytest.raises(ValueError, match="obsid"):
        jobs.filter(obs_ids=[1])


@pytest.mark.usefixtures("mock_login")
def test_all_ready_checks_the_given_jobs(host: str, serve_jobs: Callable[..., None]) -> None:
    """all_ready is True, False, or raises AsvoError with the variant's fields."""
    serve_jobs(mixed_jobs())
    jobs = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert jobs.all_ready([JOB_ID_READY])
    assert not jobs.all_ready([JOB_ID_READY, JOB_ID_QUEUED])

    with pytest.raises(gs.AsvoError) as failed:
        jobs.all_ready([JOB_ID_FAILED])
    assert failed.value.kind == "JobFailed"
    assert failed.value.job_id == JOB_ID_FAILED
    assert failed.value.error == "the conversion failed"

    with pytest.raises(gs.AsvoError) as missing:
        jobs.all_ready([999])
    assert missing.value.kind == "NoAsvoJob"


@pytest.mark.usefixtures("mock_login")
def test_json_is_keyed_by_job_id(host: str, serve_jobs: Callable[..., None]) -> None:
    """json() gives the same shape as `giant-squid list --json`."""
    serve_jobs(mixed_jobs())
    jobs = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    parsed = json.loads(jobs.json())

    assert parsed[str(JOB_ID_READY)]["job_id"] == JOB_ID_READY


def test_an_empty_api_key_is_rejected_before_any_request(host: str, httpserver: HTTPServer) -> None:
    """An empty API key raises AsvoApiError with kind MissingAuthKey, and nothing is sent."""
    with pytest.raises(gs.AsvoApiError) as err:
        gs.AsvoClient(host, "")
    assert err.value.kind == "MissingAuthKey"
    assert len(httpserver.log) == 0


def test_a_rejected_login_raises_an_authentication_failure(host: str, httpserver: HTTPServer) -> None:
    """A failed login raises AsvoApiError with the server's message."""
    httpserver.expect_request(LOGIN_PATH, method="POST").respond_with_data("invalid api key", status=401)

    with pytest.raises(gs.AsvoApiError) as err:
        gs.AsvoClient(host, TEST_API_KEY)

    assert err.value.kind == "AuthenticationFailed"
    assert "invalid api key" in err.value.message


@pytest.mark.usefixtures("mock_login")
def test_a_structured_server_error_has_its_fields(host: str, httpserver: HTTPServer) -> None:
    """A structured error response raises AsvoApiError with error_code, message, detail and suggestion."""
    httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_json(
        error_response("JOB_INVALID_STATE", "Job is not ready"), status=400
    )
    client = gs.AsvoClient(host, TEST_API_KEY)

    with pytest.raises(gs.AsvoApiError) as err:
        client.get_jobs()

    assert err.value.kind == "ApiError"
    assert err.value.error_code == "JOB_INVALID_STATE"
    assert err.value.message == "Job is not ready"
    assert err.value.suggestion == "try again"


@pytest.mark.usefixtures("mock_login")
def test_a_token_cache_path_lets_a_second_client_skip_the_login(
    host: str, httpserver: HTTPServer, tmp_path: pathlib.Path
) -> None:
    """With a token cache, the second client reuses the first client's session."""
    cache = tmp_path / "tokens.json"

    gs.AsvoClient(host, TEST_API_KEY, token_cache_path=cache)
    gs.AsvoClient(host, TEST_API_KEY, token_cache_path=cache)

    assert cache.exists()
    assert logins(httpserver) == 1


@pytest.mark.usefixtures("mock_login")
def test_without_a_token_cache_every_client_logs_in(host: str, httpserver: HTTPServer) -> None:
    """With no token cache, the session is in memory only."""
    for _ in range(CLIENTS):
        gs.AsvoClient(host, TEST_API_KEY)

    assert logins(httpserver) == CLIENTS


def test_a_bad_timeout_is_rejected(host: str) -> None:
    """A negative api_timeout raises ValueError."""
    with pytest.raises(ValueError, match="api_timeout"):
        gs.AsvoClient(host, TEST_API_KEY, api_timeout=-1.0)


@pytest.mark.usefixtures("mock_login")
def test_rust_log_records_reach_python_logging(host: str, caplog: pytest.LogCaptureFixture) -> None:
    """The library's warning about a non-default host arrives on a mwa_giant_squid logger."""
    with caplog.at_level(logging.WARNING, logger=MODULE_LOGGER):
        gs.AsvoClient(host, TEST_API_KEY)

    records = [r for r in caplog.records if r.name.startswith(MODULE_LOGGER)]
    assert any("non-default host" in r.getMessage() for r in records), caplog.text
    assert all(r.levelno == logging.WARNING for r in records if "non-default host" in r.getMessage())


@pytest.mark.usefixtures("mock_login")
def test_one_client_can_be_used_from_several_threads(host: str, serve_jobs: Callable[..., None]) -> None:
    """Calls release the GIL and the client is thread-safe."""
    serve_jobs(mixed_jobs())
    client = gs.AsvoClient(host, TEST_API_KEY)

    with ThreadPoolExecutor(max_workers=THREADS) as pool:
        counts = list(pool.map(lambda _: len(client.get_jobs()), range(THREADS)))

    assert counts == [len(mixed_jobs())] * THREADS


def test_the_enums_have_readable_names() -> None:
    """str() of an enum member is the library's display name."""
    assert str(gs.AsvoJobState.Ready) == "Ready"
    assert gs.AsvoJobState.Ready != gs.AsvoJobState.Queued
