"""Tests for the module functions: parse_many_jobids_or_obsids and the *_params builders.

The builders make no request. The tests that compare a builder with its submit method use the local mock MWA
ASVO, and never a real server.
"""

import errno
import inspect
import json
import os
import pathlib
from collections.abc import Callable
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, TEST_OBSID

# A job ID (not 10 digits, so not an obsid) and a second obsid.
TEST_JOBID = 12345
OTHER_OBSID = 1090008640

# The job the mock says it created.
NEW_JOB_ID = 777

# A conversion job to image from, and voltage job values.
SOURCE_JOB_ID = 555
VOLTAGE_OFFSET = 10
VOLTAGE_DURATION = 32

# Each builder, its submit method, its endpoint, the positional arguments and some keyword arguments that
# differ from the schema defaults.
BUILDERS: list[tuple[str, str, str, tuple[Any, ...], dict[str, Any]]] = [
    (
        "download_vis_job_params",
        "submit_download_vis_job",
        "/api/v2/download_vis_job",
        (TEST_OBSID,),
        {"delivery": gs.Delivery.Dug, "allow_resubmit": True},
    ),
    (
        "download_meta_job_params",
        "submit_download_meta_job",
        "/api/v2/download_vis_job",
        (TEST_OBSID,),
        {"delivery_format": gs.DeliveryFormat.Files},
    ),
    (
        "conversion_job_params",
        "submit_conversion_job",
        "/api/v2/conversion_job",
        (TEST_OBSID,),
        {"output": gs.Output.Uvfits, "avg_freq_res": 10.0, "centre": gs.Centre.Custom, "custom_centre_ra": 12.5},
    ),
    (
        "imaging_job_params",
        "submit_imaging_job",
        "/api/v2/imaging_job",
        (TEST_OBSID,),
        {"pol": gs.Polarization.Xx, "image_size": 1024, "wstack_nwlayers": 64, "weighting": gs.Weighting.Natural},
    ),
    (
        "image_from_job_params",
        "submit_image_from_job",
        "/api/v2/image_from_job",
        (TEST_OBSID, SOURCE_JOB_ID),
        {"pol": "YY", "output_mode": gs.OutputMode.AllFiles, "nmiter": 7},
    ),
    (
        "voltage_job_params",
        "submit_voltage_job",
        "/api/v2/voltage_job",
        (TEST_OBSID, VOLTAGE_OFFSET, VOLTAGE_DURATION),
        {"from_channel": 3, "to_channel": 9},
    ),
    (
        "beamformer_job_params",
        "submit_beamformer_job",
        "/api/v2/beamformer_job",
        (TEST_OBSID,),
        {"delivery": gs.Delivery.Scratch},
    ),
]
BUILDER_IDS = [builder for builder, *_ in BUILDERS]


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


def test_parse_sorts_job_ids_from_obsids() -> None:
    """A valid obsid is an obsid; any other integer is a job ID. The order is kept."""
    jobids, obsids = gs.parse_many_jobids_or_obsids([str(TEST_OBSID), str(TEST_JOBID), str(OTHER_OBSID)])

    assert jobids == [TEST_JOBID]
    assert obsids == [TEST_OBSID, OTHER_OBSID]


def test_parse_reads_a_file_of_job_ids_and_obsids(tmp_path: pathlib.Path) -> None:
    """A string that is not an integer is a file, separated by any whitespace."""
    listing = tmp_path / "ids.txt"
    listing.write_text(f"{TEST_OBSID} {TEST_JOBID}\n\t{OTHER_OBSID}\n")

    jobids, obsids = gs.parse_many_jobids_or_obsids([str(listing)])

    assert jobids == [TEST_JOBID]
    assert obsids == [TEST_OBSID, OTHER_OBSID]


def test_parse_rejects_text_in_a_file(tmp_path: pathlib.Path) -> None:
    """Text that is not an integer, in a file, raises ValueError that names the text."""
    listing = tmp_path / "ids.txt"
    listing.write_text(f"{TEST_OBSID} notanid\n")

    with pytest.raises(ValueError, match="notanid"):
        gs.parse_many_jobids_or_obsids([str(listing)])


def test_parse_raises_file_not_found_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """A path that does not exist raises FileNotFoundError, as Python's own file functions do."""
    with pytest.raises(FileNotFoundError):
        gs.parse_many_jobids_or_obsids([str(tmp_path / "missing.txt")])


def test_parse_of_nothing_is_nothing() -> None:
    """An empty list gives two empty lists."""
    assert gs.parse_many_jobids_or_obsids([]) == ([], [])


@pytest.mark.parametrize(("builder", "method", "path", "args", "kwargs"), BUILDERS, ids=BUILDER_IDS)
def test_a_builder_returns_the_body_its_submit_method_sends(
    client: gs.AsvoClient,
    httpserver: HTTPServer,
    builder: str,
    method: str,
    path: str,
    args: tuple[Any, ...],
    kwargs: dict[str, Any],
) -> None:
    """The dict is exactly the JSON body that the submit method sends, so a dry run shows the real request."""
    httpserver.expect_request(path, method="POST").respond_with_json(
        {"job_id": NEW_JOB_ID, "message": "ok", "status": "success"}
    )

    body = getattr(gs, builder)(*args, **kwargs)
    getattr(client, method)(*args, **kwargs)

    (request,) = [request for request, _ in httpserver.log if request.path == path]
    assert body == json.loads(request.get_data())


@pytest.mark.parametrize(("builder", "method", "path", "args", "kwargs"), BUILDERS, ids=BUILDER_IDS)
def test_a_builder_has_the_signature_of_its_submit_method(
    client: gs.AsvoClient, builder: str, method: str, path: str, args: tuple[Any, ...], kwargs: dict[str, Any]
) -> None:
    """The builder and the method take the same arguments, so neither can gain one the other lacks."""
    builder_signature = inspect.signature(getattr(gs, builder))
    method_signature = inspect.signature(getattr(client, method))

    assert builder_signature == method_signature


def test_the_download_builders_set_the_download_type() -> None:
    """The client sets download_type when it submits, so the builders set it too."""
    assert gs.download_vis_job_params(TEST_OBSID)["download_type"] == "vis"
    assert gs.download_meta_job_params(TEST_OBSID)["download_type"] == "meta"


def test_a_builder_with_no_optional_arguments_has_the_schema_defaults() -> None:
    """The module adds no defaults of its own."""
    body = gs.conversion_job_params(TEST_OBSID)

    assert body["obs_id"] == TEST_OBSID
    assert body["output"] == "ms"
    assert body["delivery"] == "acacia"
    assert "custom_centre_ra" not in body


# Invalid arguments for each kind of check: (builder, positional arguments, keyword arguments, exception).
INVALID: list[tuple[Callable[..., dict[str, Any]], tuple[Any, ...], dict[str, Any], type[Exception]]] = [
    (gs.download_vis_job_params, (1,), {}, ValueError),
    (gs.conversion_job_params, (TEST_OBSID,), {"avg_freq_res": 1281.0}, ValueError),
    (gs.imaging_job_params, (TEST_OBSID,), {"mgain": 1.5}, ValueError),
    (gs.imaging_job_params, (TEST_OBSID,), {"image_size": 100}, ValueError),
    (gs.image_from_job_params, (TEST_OBSID, 0), {}, ValueError),
    (gs.voltage_job_params, (TEST_OBSID, 5401, VOLTAGE_DURATION), {}, ValueError),
    (gs.voltage_job_params, (TEST_OBSID, VOLTAGE_OFFSET, VOLTAGE_DURATION), {"from_channel": 256}, OverflowError),
]


@pytest.mark.parametrize(("builder", "args", "kwargs", "error"), INVALID)
def test_a_builder_applies_the_submit_checks(
    builder: Callable[..., dict[str, Any]], args: tuple[Any, ...], kwargs: dict[str, Any], error: type[Exception]
) -> None:
    """A builder raises for the same bad arguments as its submit method."""
    with pytest.raises(error):
        builder(*args, **kwargs)


def test_a_missing_file_error_names_the_file(tmp_path: pathlib.Path) -> None:
    """The error has the errno, the message and the path, as Python's own file functions give them."""
    missing = tmp_path / "missing.txt"

    with pytest.raises(FileNotFoundError) as err:
        gs.parse_many_jobids_or_obsids([str(missing)])

    assert err.value.filename == str(missing)
    assert err.value.errno == errno.ENOENT
    assert err.value.strerror == os.strerror(errno.ENOENT)
    assert str(missing) in str(err.value)
