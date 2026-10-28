"""Tests for `JobDetailResponse`, `AsvoJob.detail`, `JobFile.size_bytes`, `AsvoJobVec.json` and the job type and state names."""

import json
from collections.abc import Callable
from typing import Any

import pytest

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, TEST_OBS_ID, TEST_USER_ID, job_detail

JOB_ID_FIRST = 101
JOB_ID_SECOND = 205
COMPLETED_TEXT = "2026-09-08T06:00:00"
FILE_SIZE = 1234
FILE_SHA1 = "0123456789abcdef0123456789abcdef01234567"
FILE_URL = "https://example.org/file.tar?Expires=1790000000"
# A size that the MWA ASVO does not send. The schema types the size as signed.
NEGATIVE_FILE_SIZE = -5
FAILED_ERROR_CODE = 7

# The attributes that `AsvoJob` and `JobDetailResponse` share.
SHARED_ATTRIBUTES = [
    "job_id",
    "job_type",
    "job_state",
    "error_code",
    "error_text",
    "completed",
    "created",
    "started",
    "modified",
    "user_id",
    "first_name",
    "last_name",
    "job_params",
]


def served_jobs() -> list[dict[str, Any]]:
    """A completed job with a file, then a failed job, as the server sends them. Job IDs are not in order."""
    product = {"files": [{"type": "acacia", "url": FILE_URL, "size": FILE_SIZE, "sha1": FILE_SHA1}]}
    return [
        job_detail(JOB_ID_SECOND, "error", error_code=FAILED_ERROR_CODE, error_text="the conversion failed"),
        job_detail(JOB_ID_FIRST, "completed", completed=COMPLETED_TEXT, product=product),
    ]


@pytest.fixture
def jobs(host: str, mock_login: None, serve_jobs: Callable[..., None]) -> gs.AsvoJobVec:
    """The jobs of `served_jobs`, got through a client."""
    serve_jobs(served_jobs())
    return gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY)).get_jobs()


def job_with_id(jobs: gs.AsvoJobVec, job_id: int) -> gs.AsvoJob:
    """The job that has this Job ID."""
    (job,) = jobs.filter(job_ids=[job_id])
    return job


def test_detail_is_the_job_as_the_api_gives_it(jobs: gs.AsvoJobVec) -> None:
    """`AsvoJob.detail` is a `JobDetailResponse` that has the attributes of the job."""
    job = job_with_id(jobs, JOB_ID_FIRST)

    detail = job.detail()

    assert isinstance(detail, gs.JobDetailResponse)
    for name in SHARED_ATTRIBUTES:
        assert getattr(detail, name) == getattr(job, name), name
    assert detail.job_id == JOB_ID_FIRST
    assert detail.user_id == TEST_USER_ID
    assert detail.job_state == gs.JobState.Completed


def test_the_obs_id_of_the_detail_is_the_number_that_the_api_gave(jobs: gs.AsvoJobVec) -> None:
    """The detail has the schema's `int`, and the job has the checked `ObsId`."""
    job = job_with_id(jobs, JOB_ID_FIRST)

    assert type(job.detail().obs_id) is int
    assert job.detail().obs_id == TEST_OBS_ID
    assert isinstance(job.obs_id, gs.ObsId)


def test_the_product_of_the_detail_has_the_files(jobs: gs.AsvoJobVec) -> None:
    """The product comes through the detail."""
    detail = job_with_id(jobs, JOB_ID_FIRST).detail()

    assert detail.product is not None
    (file,) = detail.product.files
    assert file.size == FILE_SIZE
    assert job_with_id(jobs, JOB_ID_SECOND).detail().product is None


def test_the_detail_repr_names_the_job(jobs: gs.AsvoJobVec) -> None:
    """The repr has the Job ID and the state."""
    text = repr(job_with_id(jobs, JOB_ID_SECOND).detail())

    assert text.startswith("JobDetailResponse(")
    assert f"job_id={JOB_ID_SECOND}" in text
    assert f"obs_id={TEST_OBS_ID}" in text


def test_size_bytes_is_the_size_as_an_unsigned_number(jobs: gs.AsvoJobVec) -> None:
    """A file has `size` and `size_bytes`."""
    product = job_with_id(jobs, JOB_ID_FIRST).product
    assert product is not None
    (file,) = product.files

    assert file.size_bytes == FILE_SIZE
    assert file.size == FILE_SIZE


@pytest.mark.usefixtures("mock_login")
def test_size_bytes_is_zero_for_a_negative_size(host: str, serve_jobs: Callable[..., None]) -> None:
    """A negative size is 0 bytes, as in the library."""
    product = {"files": [{"type": "acacia", "url": FILE_URL, "size": NEGATIVE_FILE_SIZE}]}
    serve_jobs([job_detail(JOB_ID_FIRST, "completed", completed=COMPLETED_TEXT, product=product)])

    job = gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY)).get_jobs()[0]

    assert job.product is not None
    (file,) = job.product.files
    assert file.size == NEGATIVE_FILE_SIZE
    assert file.size_bytes == 0


def test_json_is_an_object_keyed_by_job_id_in_job_id_order(jobs: gs.AsvoJobVec) -> None:
    """The server sent the second job first. The JSON is in Job ID order."""
    loaded = json.loads(jobs.json())

    assert list(loaded) == [str(JOB_ID_FIRST), str(JOB_ID_SECOND)]


def test_json_has_the_fields_that_the_api_gave(jobs: gs.AsvoJobVec) -> None:
    """Each value is the job as the API gives it, with the schema's `obs_id`."""
    loaded = json.loads(jobs.json())

    first = loaded[str(JOB_ID_FIRST)]
    assert first["job_id"] == JOB_ID_FIRST
    assert first["obs_id"] == TEST_OBS_ID
    assert first["job_state"] == "completed"
    assert first["product"]["files"][0]["size"] == FILE_SIZE
    second = loaded[str(JOB_ID_SECOND)]
    assert second["job_state"] == "error"
    assert second["error_code"] == FAILED_ERROR_CODE
    assert second["error_text"] == "the conversion failed"
    assert "product" not in second


@pytest.mark.usefixtures("mock_login")
def test_json_of_no_jobs_is_an_empty_object(host: str, serve_jobs: Callable[..., None]) -> None:
    """An empty job list is `{}`."""
    serve_jobs([])

    assert gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY)).get_jobs().json() == "{}"


@pytest.mark.parametrize("enum", [gs.JobType, gs.JobState])
def test_names_are_the_apis_values_and_parse_back(enum: Any) -> None:
    """Every member has a name, and `parse_name` reads each name as its member."""
    names = enum.names()
    members = {getattr(enum, n) for n in dir(enum) if isinstance(getattr(enum, n), enum)}

    assert all(isinstance(n, str) and n for n in names)
    assert len(names) == len(set(names)) == len(members)
    assert {enum.parse_name(n) for n in names} == members


def test_job_type_names_include_visibility() -> None:
    """The names are the lower-case values of the API."""
    assert "visibility" in gs.JobType.names()
    assert gs.JobType.parse_name("visibility") == gs.JobType.Visibility


def test_job_state_names_include_completed() -> None:
    """The names are the lower-case values of the API."""
    assert "completed" in gs.JobState.names()
    assert gs.JobState.parse_name("completed") == gs.JobState.Completed


@pytest.mark.parametrize("text", ["VISIBILITY", "Visibility", "  visibility "])
def test_parse_name_ignores_case(text: str) -> None:
    """Case does not matter, as for the command line."""
    assert gs.JobType.parse_name(text) == gs.JobType.Visibility


def test_parse_name_ignores_hyphens_and_underscores() -> None:
    """`wait-cal`, `wait_cal` and `waitcal` are one state."""
    assert gs.JobState.parse_name("wait-cal") == gs.JobState.Waitcal
    assert gs.JobState.parse_name("wait_cal") == gs.JobState.Waitcal


def test_an_unknown_job_type_raises_asvo_error() -> None:
    """The error has the kind and the text."""
    with pytest.raises(gs.AsvoError) as excinfo:
        gs.JobType.parse_name("nonsense")
    assert excinfo.value.kind == "InvalidJobType"
    assert excinfo.value.str == "nonsense"


def test_an_unknown_job_state_raises_asvo_error() -> None:
    """The error has the kind and the text."""
    with pytest.raises(gs.AsvoError) as excinfo:
        gs.JobState.parse_name("nonsense")
    assert excinfo.value.kind == "InvalidJobState"
    assert excinfo.value.str == "nonsense"


def test_only_job_type_and_job_state_have_names() -> None:
    """As in the library: the other schema enums have no `names` or `parse_name`."""
    for enum in (gs.DeliveryDestination, gs.DeliveryFormat, gs.SortBy, gs.Status, gs.Type):
        assert not hasattr(enum, "names"), enum
        assert not hasattr(enum, "parse_name"), enum
