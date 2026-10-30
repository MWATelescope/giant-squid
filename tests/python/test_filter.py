"""Tests for `JobsFilter`, and for how `AsvoClient.get_jobs` sends it."""

import datetime
from collections.abc import Callable

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import GET_JOBS_PATH, TEST_API_KEY, TEST_OBS_ID, job_detail

JOB_ID = 1234
OTHER_JOB_ID = 5678
FILTER_DAYS = 7
FROM_DATE = datetime.datetime(2026, 9, 1, tzinfo=datetime.timezone.utc)
TO_DATE = datetime.datetime(2026, 9, 8, tzinfo=datetime.timezone.utc)
TOO_MANY_DAYS = 31


def test_an_empty_filter_does_not_filter() -> None:
    """Every value is `None` or an empty list."""
    filter = gs.JobsFilter()

    assert filter.days is None
    assert filter.date_from is None
    assert filter.date_to is None
    assert filter.sort_by is None
    assert filter.job_ids == []
    assert filter.obs_ids == []
    assert filter.job_types == []
    assert filter.job_states == []
    assert repr(filter) == "JobsFilter()"


def test_the_arguments_are_keyword_only() -> None:
    """A filter has many fields, so they are named."""
    with pytest.raises(TypeError):
        gs.JobsFilter(FILTER_DAYS)  # ty: ignore[too-many-positional-arguments]


def test_a_filter_has_the_values_it_was_given() -> None:
    """Each value comes back as it went in. An Obs ID comes back as an `ObsId`."""
    filter = gs.JobsFilter(
        days=FILTER_DAYS,
        sort_by=gs.SortBy.Id,
        job_ids=[JOB_ID],
        job_types=[gs.JobType.Imaging],
        job_states=[gs.JobState.Queued],
    )

    assert filter.days == FILTER_DAYS
    assert filter.sort_by == gs.SortBy.Id
    assert filter.job_ids == [JOB_ID]
    assert filter.job_types == [gs.JobType.Imaging]
    assert filter.job_states == [gs.JobState.Queued]
    obs_filter = gs.JobsFilter(obs_ids=[TEST_OBS_ID, gs.ObsId(TEST_OBS_ID)])
    assert obs_filter.obs_ids == [TEST_OBS_ID, TEST_OBS_ID]
    assert all(isinstance(o, gs.ObsId) for o in obs_filter.obs_ids)


def test_the_dates_come_back_as_aware_datetimes() -> None:
    """The dates are in UTC."""
    filter = gs.JobsFilter(date_from=FROM_DATE, date_to=TO_DATE)

    assert filter.date_from == FROM_DATE
    assert filter.date_to == TO_DATE


def test_a_filter_can_be_changed() -> None:
    """The attributes are the fields of the Rust struct, and they can be set."""
    filter = gs.JobsFilter(days=FILTER_DAYS)

    filter.days = None
    filter.job_ids = [JOB_ID, OTHER_JOB_ID]
    filter.obs_ids = [TEST_OBS_ID]
    filter.sort_by = gs.SortBy.Id
    filter.date_from = FROM_DATE

    assert filter.days is None
    assert filter.job_ids == [JOB_ID, OTHER_JOB_ID]
    assert filter.obs_ids == [TEST_OBS_ID]
    assert filter.sort_by == gs.SortBy.Id
    assert filter.date_from == FROM_DATE


def test_the_repr_shows_the_values_that_are_set() -> None:
    """The repr is a call that makes the same filter."""
    text = repr(gs.JobsFilter(days=FILTER_DAYS, job_ids=[JOB_ID, OTHER_JOB_ID], job_types=[gs.JobType.Imaging]))

    assert text.startswith("JobsFilter(days=7, ")
    assert f"job_ids=[{JOB_ID}, {OTHER_JOB_ID}]" in text


@pytest.mark.parametrize("days", [0, -1, TOO_MANY_DAYS])
def test_days_outside_the_limits_are_refused_when_they_are_set(days: int) -> None:
    """The check is the library's `validate::days`."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="days"):
        gs.JobsFilter(days=days)
    filter = gs.JobsFilter()
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="days"):
        filter.days = days


def test_a_job_id_of_zero_is_refused() -> None:
    """0 is not a Job ID."""
    with pytest.raises(ValueError, match="0 is not a Job ID"):
        gs.JobsFilter(job_ids=[0])


def test_an_obs_id_that_is_not_valid_is_refused() -> None:
    """The error is `ObsIdError`, as everywhere."""
    with pytest.raises(gs.ObsIdError):
        gs.JobsFilter(obs_ids=[1])


def test_validate_accepts_a_good_filter() -> None:
    """A filter by days, or by dates, is fine."""
    gs.JobsFilter(days=FILTER_DAYS).validate()
    gs.JobsFilter(date_from=FROM_DATE, date_to=TO_DATE).validate()


def test_validate_refuses_job_ids_with_obs_ids() -> None:
    """The jobs are selected by Job IDs or by Obs IDs."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="Job IDs and Obs IDs"):
        gs.JobsFilter(job_ids=[JOB_ID], obs_ids=[TEST_OBS_ID]).validate()


def test_validate_refuses_days_with_a_date() -> None:
    """The jobs are selected by `days` or by the dates."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="date_from or date_to"):
        gs.JobsFilter(days=FILTER_DAYS, date_from=FROM_DATE).validate()


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_with_no_filter_sends_the_default_request(
    host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """`get_jobs()` is `get_jobs(JobsFilter())`."""
    serve_jobs([job_detail(JOB_ID, "completed")])
    client = gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))

    plain = client.get_jobs()
    empty = client.get_jobs(gs.JobsFilter())

    assert [j.job_id for j in plain] == [j.job_id for j in empty] == [JOB_ID]
    bodies = [r.get_json() for r, _ in httpserver.log if r.path == GET_JOBS_PATH]
    assert len(bodies) == 2
    assert bodies[0] == bodies[1]


@pytest.mark.usefixtures("mock_login")
def test_get_jobs_refuses_a_filter_that_is_not_valid_and_sends_nothing(
    host: str, httpserver: HTTPServer, serve_jobs: Callable[..., None]
) -> None:
    """Job IDs and Obs IDs together: the check is before any request."""
    serve_jobs([])
    client = gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))
    before = len(httpserver.log)
    filter = gs.JobsFilter(job_ids=[JOB_ID])
    filter.obs_ids = [TEST_OBS_ID]

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="Job IDs and Obs IDs"):
        client.get_jobs(filter)

    assert len(httpserver.log) == before
