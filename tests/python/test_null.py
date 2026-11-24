"""Tests that ``None`` is sent as ``null``, and that a default argument is the schema's default.

``None`` never means "use the MWA ASVO's default": the default is the value of the argument when it is left out.
The mock MWA ASVO receives the request body, because that is what the MWA ASVO acts on.
"""

import json
from collections.abc import Callable
from functools import partial
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import IMAGE_FROM_JOB, IMAGING_JOB, TEST_API_KEY, TEST_OBS_ID
from .test_submit import (
    CONVERSION_PATH,
    DOWNLOAD_PATH,
    IMAGE_FROM_JOB_PATH,
    IMAGING_PATH,
    SOURCE_JOB_ID,
    VOLTAGE_DURATION,
    VOLTAGE_OFFSET,
    VOLTAGE_PATH,
    body_sent_to,
    submit,
    submitted,
)


@pytest.fixture
def client(host: str, mock_login: None) -> gs.MwaAsvoClient:
    """A client that has logged in to the mock.

    Args:
        host: The mock's base URL.
        mock_login: Serves the login.

    Returns:
        The client.
    """
    return gs.MwaAsvoClient(gs.MwaAsvoClientConfig(host, TEST_API_KEY))


# A voltage job has an offset and a duration.
VOLTAGE_JOB = partial(gs.DownloadJobParams, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION)
METADATA_JOB = partial(gs.DownloadJobParams, only_metadata=True)

# Each case: the class (or a function that makes it), its endpoint, its positional arguments, the argument that is set
# to None, and the value that the argument has when it is left out (the schema's default).
NULLABLE_WITH_DEFAULT = [
    (gs.DownloadJobParams, DOWNLOAD_PATH, (TEST_OBS_ID,), "allow_resubmit", False),
    (METADATA_JOB, DOWNLOAD_PATH, (TEST_OBS_ID,), "allow_resubmit", False),
    (gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), "allow_resubmit", False),
    (gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), "avg_freq_res", 40.0),
    (gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), "avg_time_res", 2.0),
    (gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), "no_rfi", False),
    (gs.ConversionJobParams, CONVERSION_PATH, (TEST_OBS_ID,), "no_geometric_delay", False),
    (IMAGING_JOB, IMAGING_PATH, (TEST_OBS_ID,), "allow_resubmit", False),
    (IMAGING_JOB, IMAGING_PATH, (TEST_OBS_ID,), "avg_freq_res", 40.0),
    (IMAGING_JOB, IMAGING_PATH, (TEST_OBS_ID,), "abs_threshold", 0.001),
    (IMAGING_JOB, IMAGING_PATH, (TEST_OBS_ID,), "no_flag_dc", False),
    (IMAGE_FROM_JOB, IMAGE_FROM_JOB_PATH, (SOURCE_JOB_ID,), "allow_resubmit", False),
    (IMAGE_FROM_JOB, IMAGE_FROM_JOB_PATH, (SOURCE_JOB_ID,), "abs_threshold", 0.001),
    (VOLTAGE_JOB, VOLTAGE_PATH, (TEST_OBS_ID,), "allow_resubmit", False),
]


@pytest.mark.parametrize(("job", "path", "args", "argument", "default"), NULLABLE_WITH_DEFAULT)
def test_none_is_sent_as_null_and_the_default_is_the_schema_default(
    httpserver: HTTPServer,
    client: gs.MwaAsvoClient,
    job: Callable[..., Any],
    path: str,
    args: tuple[Any, ...],
    argument: str,
    default: object,
) -> None:
    """``argument=None`` sends ``null``; leaving it out sends the schema's default.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
        job: The class of the request body.
        path: Its endpoint.
        args: Its positional arguments.
        argument: The argument to set to ``None``.
        default: The schema's default of the argument.
    """
    submitted(httpserver, path)
    submit(client, job(*args))
    submit(client, job(*args, **{argument: None}))

    left_out, none = [json.loads(r.get_data()) for r, _ in httpserver.log if r.path == path]
    assert left_out[argument] == default
    assert none.get(argument, "absent") is None


def test_none_in_an_argument_whose_default_is_null_leaves_it_out(
    httpserver: HTTPServer,
    client: gs.MwaAsvoClient,
) -> None:
    """The default of ``custom_centre_ra`` is ``null``, and the body leaves a null field out.

    Args:
        httpserver: The mock server.
        client: A logged-in client.
    """
    submitted(httpserver, CONVERSION_PATH)
    client.submit_preprocess_job(gs.ConversionJobParams(TEST_OBS_ID, custom_centre_ra=None))
    assert "custom_centre_ra" not in body_sent_to(httpserver, CONVERSION_PATH)
