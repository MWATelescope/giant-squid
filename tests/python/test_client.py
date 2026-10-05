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
    TEST_USER_ID,
    error_response,
    job_detail,
)

# The schema's default for the days of a job listing, read from the schema that the library is built from, so
# that the test follows the API and does not repeat its number.
SCHEMA_PATH = pathlib.Path(__file__).parents[2] / "src" / "asvo" / "apiv2" / "openapi-schema.json"
SCHEMA_DEFAULT_DAYS = json.loads(SCHEMA_PATH.read_text())["definitions"]["JobsByUserRequest"]["properties"]["days"][
    "default"
]

# Job IDs used by these tests.
JOB_ID_READY = 101
JOB_ID_QUEUED = 102
JOB_ID_FAILED = 103
# The server's error code for the failed job.
FAILED_ERROR_CODE = 7

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
        job_detail(JOB_ID_FAILED, "error", error_code=FAILED_ERROR_CODE, error_text="the conversion failed"),
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
    assert ready.job_type == gs.JobType.Visibility
    assert ready.job_state == gs.JobState.Completed
    assert ready.error_text is None
    assert ready.error_code is None
    assert ready.completed == COMPLETED_UTC
    assert ready.product is not None
    (file,) = ready.product.files
    assert file.type == gs.Delivery.Acacia
    assert file.url == FILE_URL
    assert file.size == FILE_SIZE
    assert file.sha1 == FILE_SHA1
    assert file.path is None

    assert jobs[1].job_state == gs.JobState.Queued
    assert jobs[1].product is None
    assert jobs[-1].job_state == gs.JobState.Error
    assert jobs[-1].error_text == "the conversion failed"
    assert jobs[-1].error_code == FAILED_ERROR_CODE


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
    """Each filter works."""
    serve_jobs(mixed_jobs())
    jobs = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert len(jobs.filter()) == len(mixed_jobs())
    assert [j.job_id for j in jobs.filter(job_ids=[JOB_ID_QUEUED])] == [JOB_ID_QUEUED]
    assert len(jobs.filter(obs_ids=[TEST_OBS_ID])) == len(mixed_jobs())
    assert len(jobs.filter(job_types=[gs.JobType.Conversion])) == 0
    errors_and_ready = jobs.filter(job_states=[gs.JobState.Error, gs.JobState.Completed])
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
    assert failed.value.error_code == FAILED_ERROR_CODE
    assert f"has an error (code {FAILED_ERROR_CODE}): the conversion failed" in str(failed.value)

    with pytest.raises(gs.AsvoError) as missing:
        jobs.all_ready([999])
    assert missing.value.kind == "NoAsvoJob"


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
    # The message has the server's detail and suggestion as the server wrote them.
    assert str(err.value) == (
        "MWA ASVO returned an error (JOB_INVALID_STATE): Job is not ready"
        "\n  Detail: detail from the mock\n  Suggestion: try again"
    )


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


def test_every_job_type_is_a_schema_code_with_its_name() -> None:
    """Each JobType member is a schema code, in code order, and str() is the schema's name for it."""
    members = [
        gs.JobType.Conversion,
        gs.JobType.Visibility,
        gs.JobType.Metadata,
        gs.JobType.Voltage,
        gs.JobType.Cancel,
        gs.JobType.Beamformer,
        gs.JobType.Imaging,
    ]

    assert all(member == code for code, member in enumerate(members))
    assert [str(member) for member in members] == [
        "conversion",
        "visibility",
        "metadata",
        "voltage",
        "cancel",
        "beamformer",
        "imaging",
    ]


def test_the_enums_have_readable_names() -> None:
    """str() of a JobState member is the schema's value; a JobType member is its code, and str() is its name."""
    assert str(gs.JobState.Completed) == "completed"
    assert str(gs.JobState.Waitcal) == "waitcal"
    assert str(gs.JobType.Visibility) == "visibility"
    assert gs.JobType.Visibility == VISIBILITY_JOB_TYPE_NUMBER
    assert gs.JobState.Completed != gs.JobState.Queued


@pytest.mark.usefixtures("mock_login")
def test_a_file_has_the_format_the_server_gives(host: str, serve_jobs: Callable[..., None]) -> None:
    """Since schema v1.11 a product file has a format; a file without one has None."""
    product = {
        "files": [
            {"type": "acacia", "url": FILE_URL, "size": FILE_SIZE, "sha1": FILE_SHA1, "format": "tar"},
            {"type": "acacia", "url": FILE_URL, "size": FILE_SIZE, "sha1": FILE_SHA1},
        ]
    }
    serve_jobs([job_detail(JOB_ID_READY, "completed", completed=COMPLETED_TEXT, product=product)])

    (job,) = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert job.product is not None
    assert [file.format for file in job.product.files] == ["tar", None]


# The filter values of the get_jobs test, and what the server receives for them.
FILTER_DAYS = 7
# The suffix that RFC 3339 gives to UTC. Python 3.10's `fromisoformat` does not read it (3.11 and later do).
UTC_SUFFIX = "Z"


def parse_rfc3339(text: str) -> datetime.datetime:
    """Parse the time in a request body.

    Args:
        text: An RFC 3339 time, with a ``Z`` or an offset.

    Returns:
        The time, with a time zone.
    """
    return datetime.datetime.fromisoformat(
        text.removesuffix(UTC_SUFFIX) + "+00:00" if text.endswith(UTC_SUFFIX) else text
    )


FILTER_FROM = datetime.datetime(2026, 9, 1, tzinfo=datetime.timezone.utc)
FILTER_TO = datetime.datetime(2026, 9, 30, tzinfo=datetime.timezone.utc)
FILTER_SORT = "created"
IMAGING_JOB_TYPE_NUMBER = 6
# The code of the visibility download job type.
VISIBILITY_JOB_TYPE_NUMBER = 1


def get_jobs_body(httpserver: HTTPServer) -> dict[str, Any]:
    """The JSON body of the one get_jobs request the mock received.

    Args:
        httpserver: The pytest-httpserver server.

    Returns:
        The body.
    """
    (request,) = [request for request, _ in httpserver.log if request.path == GET_JOBS_PATH]
    return json.loads(request.get_data())


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_sends_every_filter_to_the_server(
    host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """Each filter reaches the request body under its OpenAPI name, with the API's own values."""
    serve_jobs([])

    gs.AsvoClient(host, TEST_API_KEY).get_jobs(
        FILTER_DAYS,
        job_state=gs.JobState.Completed,
        job_type=gs.JobType.Imaging,
        date_from=FILTER_FROM,
        date_to=FILTER_TO,
        sort_by=FILTER_SORT,
    )

    body = get_jobs_body(httpserver)
    assert body["days"] == FILTER_DAYS
    assert body["job_state"] == "completed"
    assert body["job_type"] == IMAGING_JOB_TYPE_NUMBER
    assert parse_rfc3339(body["date_from"]) == FILTER_FROM
    assert parse_rfc3339(body["date_to"]) == FILTER_TO
    assert body["sort_by"] == FILTER_SORT


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_with_no_filter_sends_none(host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None]) -> None:
    """With no filters, the body has the schema's default for days, the default order, and no other filter."""
    serve_jobs([])

    gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    body = get_jobs_body(httpserver)
    assert body["days"] == SCHEMA_DEFAULT_DAYS
    assert body["sort_by"] == "id"
    for key in ("job_state", "job_type", "date_from", "date_to"):
        assert key not in body


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_refuses_a_time_without_a_time_zone(host: str) -> None:
    """A naive datetime is ambiguous, so it is refused."""
    naive = datetime.datetime(2026, 9, 1)  # noqa: DTZ001 - naive on purpose: it must be refused
    with pytest.raises(TypeError):
        gs.AsvoClient(host, TEST_API_KEY).get_jobs(date_from=naive)


@pytest.mark.usefixtures("mock_login")
def test_a_job_has_every_field_of_the_job_detail(host: str, serve_jobs: Callable[..., None]) -> None:
    """The job detail's other fields reach AsvoJob."""
    serve_jobs([job_detail(JOB_ID_READY, "completed", started="2026-09-08T05:50:00", modified="2026-09-08T05:55:00")])

    (job,) = gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert job.created == datetime.datetime(2026, 9, 8, 5, 41, 54, 757232, tzinfo=datetime.timezone.utc)
    assert job.started == datetime.datetime(2026, 9, 8, 5, 50, tzinfo=datetime.timezone.utc)
    assert job.modified == datetime.datetime(2026, 9, 8, 5, 55, tzinfo=datetime.timezone.utc)
    assert job.user_id == TEST_USER_ID
    assert job.first_name == "Test"
    assert job.last_name == "User"
    assert job.job_params == {"obs_id": str(TEST_OBS_ID), "delivery": "acacia"}
    assert job.error_text is None


@pytest.mark.usefixtures("mock_login")
def test_an_api_error_has_its_field_errors_and_request_id(host: str, httpserver: HTTPServer) -> None:
    """field_errors is a list of dicts, request_id a string; both are in the message."""
    body = error_response("VALIDATION_ERROR", "Invalid parameters")
    body["field_errors"] = [{"field": "mgain", "message": "must be at most 1"}]
    body["request_id"] = "req-1234"
    httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_json(body, status=422)

    with pytest.raises(gs.AsvoApiError) as err:
        gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert err.value.field_errors == [{"field": "mgain", "message": "must be at most 1"}]
    assert err.value.request_id == "req-1234"
    assert "mgain: must be at most 1" in str(err.value)
    assert "req-1234" in str(err.value)


@pytest.mark.usefixtures("mock_login")
def test_an_api_error_without_details_has_empty_ones(host: str, httpserver: HTTPServer) -> None:
    """With no field errors and no request ID, the attributes are empty."""
    httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_json(
        error_response("JOB_INVALID_STATE", "Job is not ready"), status=400
    )

    with pytest.raises(gs.AsvoApiError) as err:
        gs.AsvoClient(host, TEST_API_KEY).get_jobs()

    assert err.value.field_errors == []
    assert err.value.request_id is None


def three_jobs() -> list[dict[str, Any]]:
    """A ready conversion job, a queued imaging job and a cancelled download for another obsid.

    Returns:
        The jobs, as the server sends them.
    """
    other = job_detail(3, "cancelled", job_type=1)
    other["job_params"] = {"obs_id": str(OTHER_OBS_ID), "delivery": "acacia"}
    return [job_detail(1, "completed", job_type=0), job_detail(2, "queued", job_type=6), other]


# An obsid that differs from TEST_OBS_ID.
OTHER_OBS_ID = 1090008640


@pytest.mark.usefixtures("mock_login")
def test_list_jobs_filters_by_several_states_and_obsids(host: str, serve_jobs: Callable[..., None]) -> None:
    """The lists are applied to the result."""
    serve_jobs(three_jobs())
    client = gs.AsvoClient(host, TEST_API_KEY)

    completed_or_queued = client.list_jobs(job_states=[gs.JobState.Completed, gs.JobState.Queued])
    cancelled = client.list_jobs(job_states=[gs.JobState.Cancelled])
    other_obs = client.list_jobs(obs_ids=[OTHER_OBS_ID])

    assert [job.job_id for job in completed_or_queued] == [1, 2]
    assert [job.job_id for job in cancelled] == [3]
    assert [job.job_id for job in other_obs] == [3]


@pytest.mark.usefixtures("mock_login")
def test_list_jobs_sends_a_single_state_and_type_to_the_server(
    host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """One supported state and one type are server filters, so less comes back."""
    serve_jobs(three_jobs())

    jobs = gs.AsvoClient(host, TEST_API_KEY).list_jobs(
        job_states=[gs.JobState.Queued], job_types=[gs.JobType.Imaging], days=FILTER_DAYS
    )

    body = get_jobs_body(httpserver)
    assert body["job_state"] == "queued"
    assert body["job_type"] == IMAGING_JOB_TYPE_NUMBER
    assert body["days"] == FILTER_DAYS
    assert [job.job_id for job in jobs] == [2]


@pytest.mark.usefixtures("mock_login")
@pytest.mark.parametrize("days", [-1, 0, 31])
def test_days_outside_one_to_thirty_raise_value_error_before_any_request(
    host: str, httpserver: HTTPServer, days: int
) -> None:
    """`get_jobs` and `list_jobs` take the schema's 1 to 30 for ``days``, and the message gives the limits."""
    client = gs.AsvoClient(host, TEST_API_KEY)
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="days.*between 1 and 30"):
        client.get_jobs(days)
    with pytest.raises(ValueError, match="days.*between 1 and 30"):
        client.list_jobs(days=days)

    assert len(httpserver.log) == requests_before


@pytest.mark.usefixtures("mock_login")
@pytest.mark.parametrize("days", [1, 30])
def test_the_ends_of_the_days_range_are_sent(
    host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None], days: int
) -> None:
    """``days`` of 1 and of 30 are accepted and sent as they are."""
    serve_jobs([])
    gs.AsvoClient(host, TEST_API_KEY).get_jobs(days)

    assert get_jobs_body(httpserver)["days"] == days


@pytest.mark.usefixtures("mock_login")
def test_list_jobs_refuses_job_ids_and_obs_ids_together(host: str, httpserver: HTTPServer) -> None:
    """Job IDs and obsids together raise ValueError before any listing."""
    client = gs.AsvoClient(host, TEST_API_KEY)
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="can't specify both"):
        client.list_jobs(job_ids=[1], obs_ids=[TEST_OBS_ID])

    assert len(httpserver.log) == requests_before
