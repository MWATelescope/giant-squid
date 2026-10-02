"""Tests for the module functions: parse_many_job_ids_or_obs_ids and the *_params builders.

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

from .conftest import TEST_API_KEY, TEST_OBS_ID
from .test_client import SCHEMA_PATH

# A job ID (not 10 digits, so not an obsid) and a second obsid.
TEST_JOB_ID = 12345
OTHER_OBS_ID = 1090008640

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
        (TEST_OBS_ID,),
        {"delivery": gs.Delivery.Dug, "allow_resubmit": True},
    ),
    (
        "download_meta_job_params",
        "submit_download_meta_job",
        "/api/v2/download_vis_job",
        (TEST_OBS_ID,),
        {"delivery_format": gs.DeliveryFormat.Files},
    ),
    (
        "conversion_job_params",
        "submit_conversion_job",
        "/api/v2/conversion_job",
        (TEST_OBS_ID,),
        {"output": gs.Output.Uvfits, "avg_freq_res": 10.0, "centre": gs.Centre.Custom, "custom_centre_ra": 12.5},
    ),
    (
        "imaging_job_params",
        "submit_imaging_job",
        "/api/v2/imaging_job",
        (TEST_OBS_ID,),
        {"pol": gs.Polarization.Xx, "image_size": 1024, "wstack_nwlayers": 64, "weighting": gs.Weighting.Natural},
    ),
    (
        "image_from_job_params",
        "submit_image_from_job",
        "/api/v2/image_from_job",
        (TEST_OBS_ID, SOURCE_JOB_ID),
        {"pol": gs.Polarization.Yy, "output_mode": gs.OutputMode.AllFiles, "nmiter": 7},
    ),
    (
        "voltage_job_params",
        "submit_voltage_job",
        "/api/v2/voltage_job",
        (TEST_OBS_ID, VOLTAGE_OFFSET, VOLTAGE_DURATION),
        {"from_channel": 3, "to_channel": 9},
    ),
    (
        "beamformer_job_params",
        "submit_beamformer_job",
        "/api/v2/beamformer_job",
        (TEST_OBS_ID,),
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


def test_parse_sorts_job_ids_from_obs_ids() -> None:
    """A valid obsid is an obsid; any other integer is a job ID. The order is kept."""
    job_ids, obs_ids = gs.parse_many_job_ids_or_obs_ids([str(TEST_OBS_ID), str(TEST_JOB_ID), str(OTHER_OBS_ID)])

    assert job_ids == [TEST_JOB_ID]
    assert obs_ids == [TEST_OBS_ID, OTHER_OBS_ID]


def test_parse_reads_a_file_of_job_ids_and_obs_ids(tmp_path: pathlib.Path) -> None:
    """A string that is not an integer is a file, separated by any whitespace."""
    listing = tmp_path / "ids.txt"
    listing.write_text(f"{TEST_OBS_ID} {TEST_JOB_ID}\n\t{OTHER_OBS_ID}\n")

    job_ids, obs_ids = gs.parse_many_job_ids_or_obs_ids([str(listing)])

    assert job_ids == [TEST_JOB_ID]
    assert obs_ids == [TEST_OBS_ID, OTHER_OBS_ID]


def test_parse_rejects_text_in_a_file(tmp_path: pathlib.Path) -> None:
    """Text that is not an integer, in a file, raises ValueError that names the text."""
    listing = tmp_path / "ids.txt"
    listing.write_text(f"{TEST_OBS_ID} notanid\n")

    with pytest.raises(ValueError, match="notanid"):
        gs.parse_many_job_ids_or_obs_ids([str(listing)])


def test_parse_raises_file_not_found_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """A path that does not exist raises FileNotFoundError, as Python's own file functions do."""
    with pytest.raises(FileNotFoundError):
        gs.parse_many_job_ids_or_obs_ids([str(tmp_path / "missing.txt")])


def test_parse_of_nothing_is_nothing() -> None:
    """An empty list gives two empty lists."""
    assert gs.parse_many_job_ids_or_obs_ids([]) == ([], [])


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


@pytest.mark.parametrize(("builder", "method", "path", "args", "kwargs"), BUILDERS, ids=BUILDER_IDS)
def test_staging_count_is_not_an_argument_and_not_in_a_body(
    client: gs.AsvoClient, builder: str, method: str, path: str, args: tuple[Any, ...], kwargs: dict[str, Any]
) -> None:
    """``staging_count`` is for the MWA ASVO's processors, and the API will remove it: it is never sent or taken."""
    names = [
        *inspect.signature(getattr(gs, builder)).parameters,
        *inspect.signature(getattr(client, method)).parameters,
    ]

    assert not [name for name in names if "staging" in name]
    assert "staging_count" not in getattr(gs, builder)(*args, **kwargs)


# Each builder's request body, by the schema that defines it.
BODY_SCHEMAS = {
    "download_vis_job_params": "DownloadJobParams",
    "download_meta_job_params": "DownloadJobParams",
    "conversion_job_params": "ConversionJobParams",
    "imaging_job_params": "ImagingJobFlow1Params",
    "image_from_job_params": "ImagingJobFlow2Params",
    "voltage_job_params": "VoltageJobParams",
    "beamformer_job_params": "BeamformerJobParams",
}


@pytest.mark.parametrize(("builder", "method", "path", "args", "kwargs"), BUILDERS, ids=BUILDER_IDS)
def test_every_field_of_a_body_is_in_the_schema(
    builder: str, method: str, path: str, args: tuple[Any, ...], kwargs: dict[str, Any]
) -> None:
    """Only parameters that the API defines are sent: no `flags`, and nothing else the schema does not have."""
    schema = json.loads(SCHEMA_PATH.read_text())["definitions"]
    properties = schema[BODY_SCHEMAS[builder]]["properties"]

    body = getattr(gs, builder)(*args, **kwargs)

    assert not [key for key in body if key not in properties]
    assert "flags" not in body


def test_the_download_builders_set_the_download_type() -> None:
    """The client sets download_type when it submits, so the builders set it too."""
    assert gs.download_vis_job_params(TEST_OBS_ID)["download_type"] == "vis"
    assert gs.download_meta_job_params(TEST_OBS_ID)["download_type"] == "meta"


def test_a_builder_with_no_optional_arguments_has_the_schema_defaults() -> None:
    """The module adds no defaults of its own."""
    body = gs.conversion_job_params(TEST_OBS_ID)

    assert body["obs_id"] == TEST_OBS_ID
    assert body["output"] == "ms"
    assert body["delivery"] == "acacia"
    assert "custom_centre_ra" not in body


# Invalid arguments for each kind of check: (builder, positional arguments, keyword arguments, exception).
INVALID: list[tuple[Callable[..., dict[str, Any]], tuple[Any, ...], dict[str, Any], type[Exception]]] = [
    (gs.download_vis_job_params, (1,), {}, ValueError),
    (gs.conversion_job_params, (TEST_OBS_ID,), {"avg_freq_res": 1281.0}, ValueError),
    (gs.imaging_job_params, (TEST_OBS_ID,), {"mgain": 1.5}, ValueError),
    (gs.imaging_job_params, (TEST_OBS_ID,), {"image_size": 100}, ValueError),
    (gs.image_from_job_params, (TEST_OBS_ID, 0), {}, ValueError),
    (gs.voltage_job_params, (TEST_OBS_ID, 5401, VOLTAGE_DURATION), {}, ValueError),
    (gs.voltage_job_params, (TEST_OBS_ID, VOLTAGE_OFFSET, VOLTAGE_DURATION), {"from_channel": 256}, OverflowError),
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
        gs.parse_many_job_ids_or_obs_ids([str(missing)])

    assert err.value.filename == str(missing)
    assert err.value.errno == errno.ENOENT
    assert err.value.strerror == os.strerror(errno.ENOENT)
    assert str(missing) in str(err.value)
