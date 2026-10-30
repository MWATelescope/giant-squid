"""Tests of two rules of the MWA ASVO that the module refuses before it sends a request.

* The jobs of ``get_jobs`` are selected by ``days`` or by ``date_from`` and ``date_to``, not by both.
  With a date and no ``days``, the request has ``"days": null``.
* Acacia delivers a tar only: ``delivery_format`` must be ``Tar`` when ``delivery`` is ``Acacia`` (the default).
"""

import datetime
import json
from collections.abc import Callable
from functools import partial
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import GET_JOBS_PATH, TEST_API_KEY, TEST_OBS_ID
from .test_client import SCHEMA_PATH
from .test_submit import (
    CONVERSION_PATH,
    DOWNLOAD_PATH,
    IMAGE_FROM_JOB_PATH,
    IMAGING_PATH,
    SOURCE_JOB_ID,
    submit,
    submitted,
)

# A time with a time zone, for the dates.
DATE_FROM = datetime.datetime(2026, 9, 1, tzinfo=datetime.timezone.utc)
DATE_TO = datetime.datetime(2026, 9, 30, tzinfo=datetime.timezone.utc)
# The same times as the MWA ASVO receives them.
DATE_FROM_JSON = "2026-09-01T00:00:00Z"
DATE_TO_JSON = "2026-09-30T00:00:00Z"

# The `days` that the tests give.
DAYS = 7

# Each class that has a delivery and a format (a function that makes it), its endpoint and its positional arguments.
DELIVERY_JOBS = [
    pytest.param(gs.DownloadJobParams, DOWNLOAD_PATH, (TEST_OBS_ID,), id="vis"),
    pytest.param(partial(gs.DownloadJobParams, only_get_metadata=True), DOWNLOAD_PATH, (TEST_OBS_ID,), id="meta"),
    pytest.param(gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), id="conversion"),
    pytest.param(gs.ImagingJobFlow1Params, IMAGING_PATH, (TEST_OBS_ID,), id="imaging"),
    pytest.param(gs.ImagingJobFlow2Params, IMAGE_FROM_JOB_PATH, (SOURCE_JOB_ID,), id="image_from_job"),
]


@pytest.fixture
def client(host: str, mock_login: None) -> gs.AsvoClient:
    """A client that has logged in to the mock.

    Args:
        host: The mock's base URL.
        mock_login: Serves the login.

    Returns:
        The client.
    """
    return gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))


def requests_to(httpserver: HTTPServer, path: str) -> list[dict[str, Any]]:
    """The JSON bodies of the requests that the mock received at ``path``.

    Args:
        httpserver: The mock.
        path: The endpoint.

    Returns:
        The bodies, in order.
    """
    return [json.loads(request.get_data()) for request, _ in httpserver.log if request.path == path]


@pytest.mark.parametrize(("job", "path", "args"), DELIVERY_JOBS)
def test_acacia_with_files_is_refused_before_any_request(
    httpserver: HTTPServer, client: gs.AsvoClient, job: Callable[..., Any], path: str, args: tuple[Any, ...]
) -> None:
    """Every submit method refuses ``Files`` with ``Acacia`` (given, or the default), and sends nothing.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
        job: The class of the request body.
        path: Its endpoint.
        args: Its positional arguments.
    """
    submitted(httpserver, path)

    for delivery in ({}, {"delivery": gs.DeliveryDestination.Acacia}):
        params = job(*args, delivery_format=gs.DeliveryFormat.Files, **delivery)
        with pytest.raises(
            gs.AsvoApiErrorInvalidParameter, match="delivery_format: must be tar when delivery is acacia"
        ):
            submit(client, params)
    assert requests_to(httpserver, path) == []


@pytest.mark.parametrize(("job", "path", "args"), DELIVERY_JOBS)
def test_the_other_deliveries_and_formats_are_sent(
    httpserver: HTTPServer, client: gs.AsvoClient, job: Callable[..., Any], path: str, args: tuple[Any, ...]
) -> None:
    """Acacia with tar, and the other deliveries with either format, are sent as they are given.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
        job: The class of the request body.
        path: Its endpoint.
        args: Its positional arguments.
    """
    submitted(httpserver, path)
    combinations = [
        (gs.DeliveryDestination.Acacia, gs.DeliveryFormat.Tar),
        (gs.DeliveryDestination.Scratch, gs.DeliveryFormat.Files),
        (gs.DeliveryDestination.Scratch, gs.DeliveryFormat.Tar),
        (gs.DeliveryDestination.Dug, gs.DeliveryFormat.Files),
        (gs.DeliveryDestination.Dug, gs.DeliveryFormat.Tar),
    ]
    for delivery, delivery_format in combinations:
        submit(client, job(*args, delivery=delivery, delivery_format=delivery_format))

    bodies = requests_to(httpserver, path)
    assert [(body["delivery"], body["delivery_format"]) for body in bodies] == [
        (str(delivery), str(delivery_format)) for delivery, delivery_format in combinations
    ]


def expect_get_jobs(httpserver: HTTPServer) -> None:
    """Serve an empty job list.

    Args:
        httpserver: The mock server.
    """
    httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_json({"jobs": [], "total_count": 0})


@pytest.mark.parametrize(("date_from", "date_to"), [(DATE_FROM, None), (None, DATE_TO), (DATE_FROM, DATE_TO)])
def test_days_with_a_date_is_refused_before_any_request(
    httpserver: HTTPServer,
    client: gs.AsvoClient,
    date_from: datetime.datetime | None,
    date_to: datetime.datetime | None,
) -> None:
    """``get_jobs`` refuses ``days`` with a date, and sends nothing.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
        date_from: The first date to give with ``days``, if any.
        date_to: The second date to give with ``days``, if any.
    """
    expect_get_jobs(httpserver)

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="days: can't be used with date_from or date_to"):
        client.get_jobs(gs.JobsFilter(days=DAYS, date_from=date_from, date_to=date_to))
    assert requests_to(httpserver, GET_JOBS_PATH) == []


def test_a_date_sends_null_days_and_days_alone_sends_no_date(httpserver: HTTPServer, client: gs.AsvoClient) -> None:
    """With a date and no ``days``, ``days`` is ``null``. With ``days`` alone, there is no date. With neither, ``days`` is the default.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
    """
    schema_default_days = json.loads(SCHEMA_PATH.read_text())["definitions"]["JobsByUserRequest"]["properties"]["days"][
        "default"
    ]
    expect_get_jobs(httpserver)

    client.get_jobs(gs.JobsFilter(date_from=DATE_FROM, date_to=DATE_TO))
    client.get_jobs(gs.JobsFilter(date_to=DATE_TO))
    client.get_jobs(gs.JobsFilter(date_from=DATE_FROM))
    client.get_jobs(gs.JobsFilter(days=DAYS))
    client.get_jobs()

    dated, one_date, listed, by_days, neither = requests_to(httpserver, GET_JOBS_PATH)
    assert dated["days"] is None
    assert (dated["date_from"], dated["date_to"]) == (DATE_FROM_JSON, DATE_TO_JSON)
    assert one_date["days"] is None
    assert one_date["date_to"] == DATE_TO_JSON
    assert "date_from" not in one_date
    assert listed["days"] is None
    assert listed["date_from"] == DATE_FROM_JSON
    assert by_days["days"] == DAYS
    assert "date_from" not in by_days
    assert "date_to" not in by_days
    assert neither["days"] == schema_default_days
