"""Tests for the submit methods, against a local mock MWA ASVO.

Each test checks the request body the mock receives, because that is what the MWA ASVO acts on. A keyword
argument that is left out must be absent from what the module sets, so that the schema default applies: those
tests compare with the default the schema gives, never with a value the module chose.
"""

import inspect
import json
from collections.abc import Callable
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, TEST_OBS_ID, error_response
from .test_client import SCHEMA_PATH

# The endpoint of each job type.
DOWNLOAD_PATH = "/api/v2/download_vis_job"
CONVERSION_PATH = "/api/v2/conversion_job"
IMAGING_PATH = "/api/v2/imaging_job"
IMAGE_FROM_JOB_PATH = "/api/v2/image_from_job"
VOLTAGE_PATH = "/api/v2/voltage_job"
BEAMFORMER_PATH = "/api/v2/beamformer_job"

# The job the mock says it created.
NEW_JOB_ID = 777
NEW_JOB_MESSAGE = "Job submitted"

# A conversion job to image from.
SOURCE_JOB_ID = 555

# Voltage job values.
VOLTAGE_OFFSET = 10
VOLTAGE_DURATION = 32
FROM_CHANNEL = 3
TO_CHANNEL = 9

# Values the tests send that differ from the schema defaults.
FREQ_RES = 10.0
CUSTOM_RA = 12.5
CUSTOM_DEC = -26.7
CLEAN_THRESHOLD_JY = 0.5
NMITER = 7
WSTACK_LAYERS = 64
# The schema default of clean_threshold for an image-from-job (flow 2) job, since v1.11.
FLOW2_CLEAN_THRESHOLD_DEFAULT = 0.001
VALID_IMAGE_SIZE = 1024
INVALID_IMAGE_SIZE = 100
INVALID_OBS_ID = 1


def submitted(httpserver: HTTPServer, path: str) -> None:
    """Serve a successful submission at ``path``.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.
    """
    httpserver.expect_request(path, method="POST").respond_with_json(
        {"job_id": NEW_JOB_ID, "message": NEW_JOB_MESSAGE, "status": "success"}
    )


def body_sent_to(httpserver: HTTPServer, path: str) -> dict[str, Any]:
    """The JSON body of the one request the mock received at ``path``.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.

    Returns:
        The body.
    """
    (request,) = [request for request, _ in httpserver.log if request.path == path]
    return json.loads(request.get_data())


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


def assert_response(response: gs.JobSubmittedResponse) -> None:
    """Check that ``response`` is the one that ``submitted`` serves.

    Args:
        response: What the submit method returned.
    """
    assert response.job_id == NEW_JOB_ID
    assert response.message == NEW_JOB_MESSAGE
    assert response.status == "success"


# The methods that take only delivery, delivery_format and allow_resubmit, with their endpoint.
DOWNLOAD_LIKE_METHODS = [
    ("submit_download_vis_job", DOWNLOAD_PATH),
    ("submit_download_meta_job", DOWNLOAD_PATH),
    ("submit_beamformer_job", BEAMFORMER_PATH),
]


@pytest.mark.parametrize(("method", "path"), DOWNLOAD_LIKE_METHODS)
def test_download_like_jobs_send_the_arguments_and_return_the_response(
    client: gs.AsvoClient, httpserver: HTTPServer, method: str, path: str
) -> None:
    """Each argument reaches the request body, and the reply is a JobSubmittedResponse."""
    submitted(httpserver, path)

    response = getattr(client, method)(
        TEST_OBS_ID, delivery=gs.Delivery.Dug, delivery_format=gs.DeliveryFormat.Files, allow_resubmit=True
    )

    assert_response(response)
    body = body_sent_to(httpserver, path)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["delivery"] == "dug"
    assert body["delivery_format"] == "files"
    assert body["allow_resubmit"] is True


@pytest.mark.parametrize(("method", "path"), DOWNLOAD_LIKE_METHODS)
def test_download_like_jobs_use_the_schema_defaults_when_arguments_are_left_out(
    client: gs.AsvoClient, httpserver: HTTPServer, method: str, path: str
) -> None:
    """With no optional arguments, the body has the schema's defaults."""
    submitted(httpserver, path)

    getattr(client, method)(TEST_OBS_ID)

    body = body_sent_to(httpserver, path)
    assert body["delivery"] == "acacia"
    assert body["delivery_format"] == "tar"
    assert body["allow_resubmit"] is False


def test_vis_and_meta_jobs_differ_only_in_download_type(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """Both use the download endpoint, and the library sets download_type."""
    httpserver.expect_request(DOWNLOAD_PATH, method="POST").respond_with_json(
        {"job_id": NEW_JOB_ID, "message": NEW_JOB_MESSAGE, "status": "success"}
    )

    client.submit_download_vis_job(TEST_OBS_ID)
    client.submit_download_meta_job(TEST_OBS_ID)

    types = [
        json.loads(request.get_data())["download_type"]
        for request, _ in httpserver.log
        if request.path == DOWNLOAD_PATH
    ]
    assert types == ["vis", "meta"]


def test_conversion_job_sends_the_arguments(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """Every conversion argument reaches the request body under its OpenAPI name."""
    submitted(httpserver, CONVERSION_PATH)

    response = client.submit_conversion_job(
        TEST_OBS_ID,
        delivery=gs.Delivery.Scratch,
        delivery_format=gs.DeliveryFormat.Files,
        output=gs.Output.Uvfits,
        avg_freq_res=FREQ_RES,
        avg_time_res=FREQ_RES,
        flag_edge_width=FREQ_RES,
        apply_di_cal=True,
        centre=gs.Centre.Custom,
        custom_centre_ra=CUSTOM_RA,
        custom_centre_dec=CUSTOM_DEC,
        no_apply_amps=True,
        no_digital_gains=True,
        no_flag_dc=True,
        no_geometry_delay=True,
        no_passband_gains=True,
        allow_resubmit=True,
    )

    assert_response(response)
    assert body_sent_to(httpserver, CONVERSION_PATH) == {
        "obs_id": TEST_OBS_ID,
        "delivery": "scratch",
        "delivery_format": "files",
        "output": "uvfits",
        "avg_freq_res": FREQ_RES,
        "avg_time_res": FREQ_RES,
        "flag_edge_width": FREQ_RES,
        "apply_di_cal": True,
        "centre": "custom",
        "custom_centre_ra": CUSTOM_RA,
        "custom_centre_dec": CUSTOM_DEC,
        "no_apply_amps": True,
        "no_digital_gains": True,
        "no_flag_dc": True,
        "no_geometry_delay": True,
        "no_passband_gains": True,
        "allow_resubmit": True,
        # Not given, so the schema defaults apply.
        "no_cable_delay": False,
        "no_rfi": False,
    }


def test_conversion_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """The module adds no defaults of its own; the schema's apply."""
    submitted(httpserver, CONVERSION_PATH)

    client.submit_conversion_job(TEST_OBS_ID)

    body = body_sent_to(httpserver, CONVERSION_PATH)
    assert body["output"] == "ms"
    assert body["centre"] == "phase"
    assert body["apply_di_cal"] is False
    assert "custom_centre_ra" not in body
    assert "custom_centre_dec" not in body


def test_imaging_job_sends_the_arguments(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """Enum and numeric imaging arguments reach the request body."""
    submitted(httpserver, IMAGING_PATH)

    response = client.submit_imaging_job(
        TEST_OBS_ID,
        apply_di_cal=False,
        centre=gs.Centre.Pointing,
        image_size=VALID_IMAGE_SIZE,
        nmiter=NMITER,
        output_mode=gs.OutputMode.AllFits,
        pol=gs.Polarization.Xx,
        weighting=gs.Weighting.Natural,
        uvw_max=FREQ_RES,
        wstack_nwlayers=WSTACK_LAYERS,
        clean_threshold=CLEAN_THRESHOLD_JY,
        join_polarizations=True,
        allow_resubmit=True,
    )

    assert_response(response)
    body = body_sent_to(httpserver, IMAGING_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["apply_di_cal"] is False
    assert body["centre"] == "pointing"
    assert body["image_size"] == VALID_IMAGE_SIZE
    assert body["nmiter"] == NMITER
    assert body["output_mode"] == "all_fits"
    assert body["pol"] == "XX"
    assert body["weighting"] == "natural"
    assert body["uvw_max"] == FREQ_RES
    assert body["wstack_nwlayers"] == WSTACK_LAYERS
    assert body["clean_threshold"] == CLEAN_THRESHOLD_JY
    assert body["join_polarizations"] is True
    assert body["allow_resubmit"] is True


def test_imaging_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """Optional fields with no schema default are not sent."""
    submitted(httpserver, IMAGING_PATH)

    client.submit_imaging_job(TEST_OBS_ID)

    body = body_sent_to(httpserver, IMAGING_PATH)
    assert body["pol"] == "XXYY"
    assert body["weighting"] == "briggs"
    assert body["apply_di_cal"] is True
    for name in ("custom_centre_ra", "custom_centre_dec", "uvw_max", "nwlayers", "wstack_nwlayers"):
        assert name not in body


IMAGING_SWITCHES = (
    "no_digital_gains",
    "no_flag_dc",
    "no_geometry_delay",
    "no_passband_gains",
    "no_cable_delay",
    "no_rfi",
)


# Each submit method, its endpoint, the arguments of one call, and the schema that defines its request body. The two
# tests below were the tests of the `*_params` functions (removed), which pinned these decisions for the same bodies.
BODY_CASES: list[tuple[str, str, tuple[Any, ...], dict[str, Any], str]] = [
    (
        "submit_download_vis_job",
        "/api/v2/download_vis_job",
        (TEST_OBS_ID,),
        {"allow_resubmit": True},
        "DownloadJobParams",
    ),
    (
        "submit_download_meta_job",
        "/api/v2/download_vis_job",
        (TEST_OBS_ID,),
        {"allow_resubmit": True},
        "DownloadJobParams",
    ),
    ("submit_conversion_job", "/api/v2/conversion_job", (TEST_OBS_ID,), {"avg_freq_res": 10.0}, "ConversionJobParams"),
    ("submit_imaging_job", "/api/v2/imaging_job", (TEST_OBS_ID,), {"image_size": 1024}, "ImagingJobFlow1Params"),
    ("submit_image_from_job", "/api/v2/image_from_job", (TEST_OBS_ID, 555), {"nmiter": 7}, "ImagingJobFlow2Params"),
    ("submit_voltage_job", "/api/v2/voltage_job", (TEST_OBS_ID, 10, 32), {"from_channel": 3}, "VoltageJobParams"),
    (
        "submit_beamformer_job",
        "/api/v2/beamformer_job",
        (TEST_OBS_ID,),
        {"allow_resubmit": True},
        "BeamformerJobParams",
    ),
]
BODY_CASE_IDS = [method for method, *_ in BODY_CASES]


@pytest.mark.parametrize(("method", "path", "args", "kwargs", "schema_name"), BODY_CASES, ids=BODY_CASE_IDS)
def test_every_field_of_a_body_is_in_the_schema(
    client: gs.AsvoClient,
    httpserver: HTTPServer,
    method: str,
    path: str,
    args: tuple[Any, ...],
    kwargs: dict[str, Any],
    schema_name: str,
) -> None:
    """Only parameters that the API defines are sent: no `flags`, and nothing else the schema does not have."""
    submitted(httpserver, path)
    properties = json.loads(SCHEMA_PATH.read_text())["definitions"][schema_name]["properties"]

    getattr(client, method)(*args, **kwargs)

    body = body_sent_to(httpserver, path)
    assert not [key for key in body if key not in properties]
    assert "flags" not in body


@pytest.mark.parametrize(("method", "path", "args", "kwargs", "schema_name"), BODY_CASES, ids=BODY_CASE_IDS)
def test_staging_count_is_not_an_argument_and_not_in_a_body(
    client: gs.AsvoClient,
    httpserver: HTTPServer,
    method: str,
    path: str,
    args: tuple[Any, ...],
    kwargs: dict[str, Any],
    schema_name: str,
) -> None:
    """``staging_count`` is for the MWA ASVO's processors, and the API will remove it: it is never sent or taken."""
    submitted(httpserver, path)

    getattr(client, method)(*args, **kwargs)

    assert not [name for name in inspect.signature(getattr(client, method)).parameters if "staging" in name]
    assert "staging_count" not in body_sent_to(httpserver, path)


def test_imaging_job_sends_the_six_correction_and_flagging_switches(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """Each switch reaches the body when it is given, and is ``False`` (the schema default) when it is not."""
    submitted(httpserver, IMAGING_PATH)

    client.submit_imaging_job(TEST_OBS_ID)
    default_body = body_sent_to(httpserver, IMAGING_PATH)
    client.submit_imaging_job(
        TEST_OBS_ID,
        no_digital_gains=True,
        no_flag_dc=True,
        no_geometry_delay=True,
        no_passband_gains=True,
        no_cable_delay=True,
        no_rfi=True,
    )
    given_body = [json.loads(request.get_data()) for request, _ in httpserver.log if request.path == IMAGING_PATH][-1]

    for name in IMAGING_SWITCHES:
        assert default_body[name] is False, name
        assert given_body[name] is True, name


def test_image_from_job_sends_the_source_job_and_a_free_form_pol(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """The source job ID and the arguments reach the request body."""
    submitted(httpserver, IMAGE_FROM_JOB_PATH)

    response = client.submit_image_from_job(
        TEST_OBS_ID,
        SOURCE_JOB_ID,
        pol=gs.Polarization.Yy,
        weighting=gs.Weighting.Uniform,
        output_mode=gs.OutputMode.AllFiles,
        image_size=VALID_IMAGE_SIZE,
        nmiter=NMITER,
        allow_resubmit=True,
    )

    assert_response(response)
    body = body_sent_to(httpserver, IMAGE_FROM_JOB_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["source_job_id"] == SOURCE_JOB_ID
    assert body["pol"] == "YY"
    assert body["weighting"] == "uniform"
    assert body["output_mode"] == "all_files"
    assert body["image_size"] == VALID_IMAGE_SIZE
    assert body["nmiter"] == NMITER
    assert body["allow_resubmit"] is True


def test_image_from_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """The flow 2 default for pol is the Polarization default, as for flow 1 (since schema v1.11)."""
    submitted(httpserver, IMAGE_FROM_JOB_PATH)

    client.submit_image_from_job(TEST_OBS_ID, SOURCE_JOB_ID)

    body = body_sent_to(httpserver, IMAGE_FROM_JOB_PATH)
    assert body["pol"] == "XXYY"
    assert body["clean_threshold"] == FLOW2_CLEAN_THRESHOLD_DEFAULT
    assert body["weighting"] == "briggs"


def test_voltage_job_sends_the_arguments_and_derives_channel_range(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """A channel bound sets channel_range, as in the CLI."""
    submitted(httpserver, VOLTAGE_PATH)

    response = client.submit_voltage_job(
        TEST_OBS_ID,
        VOLTAGE_OFFSET,
        VOLTAGE_DURATION,
        delivery="scratch",
        from_channel=FROM_CHANNEL,
        to_channel=TO_CHANNEL,
        allow_resubmit=True,
    )

    assert_response(response)
    body = body_sent_to(httpserver, VOLTAGE_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["offset"] == VOLTAGE_OFFSET
    assert body["duration"] == VOLTAGE_DURATION
    assert body["delivery"] == "scratch"
    assert body["from_channel"] == FROM_CHANNEL
    assert body["to_channel"] == TO_CHANNEL
    assert body["channel_range"] is True
    assert body["allow_resubmit"] is True


def test_voltage_job_without_channels_has_no_channel_range(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """With no channel bound, channel_range is false and no channel is sent."""
    submitted(httpserver, VOLTAGE_PATH)

    client.submit_voltage_job(TEST_OBS_ID, VOLTAGE_OFFSET, VOLTAGE_DURATION)

    body = body_sent_to(httpserver, VOLTAGE_PATH)
    assert body["channel_range"] is False
    assert "from_channel" not in body
    assert "to_channel" not in body
    assert body["delivery"] == "scratch"


def test_a_server_error_raises_asvo_api_error(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """An error reply to a submission has the server's error fields."""
    httpserver.expect_request(DOWNLOAD_PATH, method="POST").respond_with_json(
        error_response("JOB_ALREADY_EXISTS", "An identical job exists"), status=409
    )

    with pytest.raises(gs.AsvoApiError) as err:
        client.submit_download_vis_job(TEST_OBS_ID)

    assert err.value.kind == "ApiError"
    assert err.value.error_code == "JOB_ALREADY_EXISTS"


# Every submit method, called with only what it needs, so that one call shows an argument problem.
ALL_SUBMITS: list[tuple[str, Callable[[gs.AsvoClient, int], object]]] = [
    ("download_vis", lambda c, obs_id: c.submit_download_vis_job(obs_id)),
    ("download_meta", lambda c, obs_id: c.submit_download_meta_job(obs_id)),
    ("conversion", lambda c, obs_id: c.submit_conversion_job(obs_id)),
    ("imaging", lambda c, obs_id: c.submit_imaging_job(obs_id)),
    ("image_from_job", lambda c, obs_id: c.submit_image_from_job(obs_id, SOURCE_JOB_ID)),
    ("voltage", lambda c, obs_id: c.submit_voltage_job(obs_id, VOLTAGE_OFFSET, VOLTAGE_DURATION)),
    ("beamformer", lambda c, obs_id: c.submit_beamformer_job(obs_id)),
]


@pytest.mark.parametrize(("name", "submit"), ALL_SUBMITS, ids=[name for name, _ in ALL_SUBMITS])
def test_an_invalid_obs_id_is_rejected_before_any_submission(
    client: gs.AsvoClient, httpserver: HTTPServer, name: str, submit: Callable[[gs.AsvoClient, int], object]
) -> None:
    """Every submit method raises ValueError for a bad obsid, and sends nothing."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="obsid"):
        submit(client, INVALID_OBS_ID)

    assert len(httpserver.log) == requests_before, name


@pytest.mark.parametrize("method", ["submit_imaging_job", "submit_image_from_job"])
def test_an_invalid_image_size_is_rejected(client: gs.AsvoClient, httpserver: HTTPServer, method: str) -> None:
    """An image size the API does not accept raises ValueError, and nothing is sent."""
    args = (TEST_OBS_ID, SOURCE_JOB_ID) if method == "submit_image_from_job" else (TEST_OBS_ID,)
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="image_size"):
        getattr(client, method)(*args, image_size=INVALID_IMAGE_SIZE)

    assert len(httpserver.log) == requests_before


@pytest.mark.parametrize("method", ["submit_imaging_job", "submit_image_from_job"])
def test_a_zero_nmiter_is_rejected(client: gs.AsvoClient, method: str) -> None:
    """nmiter must be greater than zero."""
    args = (TEST_OBS_ID, SOURCE_JOB_ID) if method == "submit_image_from_job" else (TEST_OBS_ID,)

    with pytest.raises(ValueError, match="nmiter"):
        getattr(client, method)(*args, nmiter=0)


def test_a_zero_source_job_id_is_rejected(client: gs.AsvoClient) -> None:
    """source_job_id must be greater than zero."""
    with pytest.raises(ValueError, match="source_job_id"):
        client.submit_image_from_job(TEST_OBS_ID, 0)


def test_a_channel_number_out_of_range_is_rejected(client: gs.AsvoClient) -> None:
    """A voltage channel number must fit in one byte."""
    with pytest.raises(OverflowError):
        client.submit_voltage_job(TEST_OBS_ID, VOLTAGE_OFFSET, VOLTAGE_DURATION, from_channel=256)


def test_optional_arguments_must_be_given_by_keyword(client: gs.AsvoClient) -> None:
    """Optional arguments are keyword-only."""
    with pytest.raises(TypeError):
        client.submit_download_vis_job(TEST_OBS_ID, gs.Delivery.Acacia)  # ty: ignore[too-many-positional-arguments]


def test_a_delivery_of_the_wrong_type_is_rejected(client: gs.AsvoClient) -> None:
    """The delivery argument is an enum member, not a string."""
    with pytest.raises(TypeError):
        client.submit_download_vis_job(TEST_OBS_ID, delivery="acacia")  # ty: ignore[invalid-argument-type]


def test_the_new_enums_use_the_api_values_as_their_text() -> None:
    """str() of a member is the value the API uses."""
    assert str(gs.DeliveryFormat.Tar) == "tar"
    assert str(gs.Output.Uvfits) == "uvfits"
    assert str(gs.Centre.Custom) == "custom"
    assert str(gs.OutputMode.AllFits) == "all_fits"
    assert str(gs.Weighting.Briggs) == "briggs"
    assert str(gs.Polarization.Xxyy) == "XXYY"
    assert gs.Output.Ms != gs.Output.Uvfits


def test_the_response_has_a_readable_repr(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """repr() names the class and shows the job ID."""
    submitted(httpserver, DOWNLOAD_PATH)

    response = client.submit_download_vis_job(TEST_OBS_ID)

    assert repr(response).startswith("JobSubmittedResponse(")
    assert str(NEW_JOB_ID) in repr(response)


# Arguments that are outside the schema's limits: (method, keyword arguments, the name the error must give).
OUT_OF_RANGE = [
    ("submit_imaging_job", {"mgain": 1.5}, "mgain"),
    ("submit_imaging_job", {"mgain": 0.05}, "mgain"),
    ("submit_imaging_job", {"nmiter": 501}, "nmiter"),
    ("submit_imaging_job", {"auto_mask": 1}, "auto_mask"),
    ("submit_imaging_job", {"auto_threshold": 6.0}, "auto_threshold"),
    ("submit_imaging_job", {"pixel_scale": 9.0}, "pixel_scale"),
    ("submit_imaging_job", {"robust": -2.5}, "robust"),
    ("submit_imaging_job", {"clean_iterations": 1_000_001}, "clean_iterations"),
    ("submit_imaging_job", {"clean_iterations": -1}, "clean_iterations"),
    ("submit_imaging_job", {"abs_threshold": 11.0}, "abs_threshold"),
    ("submit_imaging_job", {"nwlayers": 16}, "nwlayers"),
    ("submit_imaging_job", {"uvw_max": 0.5}, "uvw_max"),
    ("submit_imaging_job", {"uvw_min": 101.0}, "uvw_min"),
    ("submit_imaging_job", {"avg_freq_res": 1281.0}, "avg_freq_res"),
    ("submit_imaging_job", {"avg_time_res": -1.0}, "avg_time_res"),
    ("submit_imaging_job", {"flag_edge_width": 641.0}, "flag_edge_width"),
    ("submit_imaging_job", {"centre": gs.Centre.Custom, "custom_centre_ra": 360.0}, "custom_centre_ra"),
    ("submit_imaging_job", {"centre": gs.Centre.Custom, "custom_centre_dec": -91.0}, "custom_centre_dec"),
    ("submit_imaging_job", {"mgain": float("nan")}, "mgain"),
    ("submit_imaging_job", {"wstack_nwlayers": 16}, "wstack_nwlayers"),
    ("submit_imaging_job", {"wstack_nwlayers": 513}, "wstack_nwlayers"),
    ("submit_image_from_job", {"mgain": 1.5}, "mgain"),
    ("submit_image_from_job", {"nmiter": 501}, "nmiter"),
    ("submit_image_from_job", {"pixel_scale": 121.0}, "pixel_scale"),
    ("submit_image_from_job", {"clean_threshold": 10.5}, "clean_threshold"),
    ("submit_image_from_job", {"nwlayers": 513}, "nwlayers"),
]


def imaging_args(method: str) -> tuple[int, ...]:
    """The positional arguments of an imaging submit method.

    Args:
        method: The method name.

    Returns:
        The obsid, and the source job ID for ``submit_image_from_job``.
    """
    return (TEST_OBS_ID, SOURCE_JOB_ID) if method == "submit_image_from_job" else (TEST_OBS_ID,)


@pytest.mark.parametrize(("method", "kwargs", "name"), OUT_OF_RANGE)
def test_an_argument_outside_the_schema_limits_is_rejected_before_any_request(
    client: gs.AsvoClient, httpserver: HTTPServer, method: str, kwargs: dict[str, Any], name: str
) -> None:
    """A number outside the schema's limits raises ValueError that names the argument, and nothing is sent."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match=name):
        getattr(client, method)(*imaging_args(method), **kwargs)

    assert len(httpserver.log) == requests_before


def test_the_error_message_gives_the_limits(client: gs.AsvoClient) -> None:
    """The message is the library's: the same text the CLI and the Rust client give."""
    with pytest.raises(ValueError) as err:
        client.submit_imaging_job(TEST_OBS_ID, mgain=1.5)

    assert str(err.value) == "Invalid mgain: must be between 0.1 and 1 (got 1.5)"


def test_the_supported_image_sizes_are_listed_in_the_error(client: gs.AsvoClient) -> None:
    """An unsupported image size is reported with the sizes that the MWA ASVO accepts."""
    with pytest.raises(ValueError, match="512, 1024, 2048, 3072, 4096, 8192"):
        client.submit_imaging_job(TEST_OBS_ID, image_size=INVALID_IMAGE_SIZE)


@pytest.mark.parametrize("method", ["submit_imaging_job", "submit_image_from_job"])
def test_values_at_the_limits_are_accepted_and_sent(client: gs.AsvoClient, httpserver: HTTPServer, method: str) -> None:
    """The ends of each range are valid."""
    path = IMAGING_PATH if method == "submit_imaging_job" else IMAGE_FROM_JOB_PATH
    submitted(httpserver, path)

    getattr(client, method)(
        *imaging_args(method), mgain=1.0, nmiter=500, pixel_scale=10.0, robust=-2.0, uvw_min=100.0, auto_mask=512
    )

    body = body_sent_to(httpserver, path)
    assert body["mgain"] == 1.0
    assert body["nmiter"] == 500
    assert body["pixel_scale"] == 10.0
    assert body["robust"] == -2.0
    assert body["uvw_min"] == 100.0
    assert body["auto_mask"] == 512


# Conversion arguments that are outside the schema's limits: (keyword arguments, the name the error must give).
CONVERSION_OUT_OF_RANGE = [
    ({"avg_freq_res": 1281.0}, "avg_freq_res"),
    ({"avg_freq_res": -1.0}, "avg_freq_res"),
    ({"avg_time_res": -1.0}, "avg_time_res"),
    ({"flag_edge_width": 641.0}, "flag_edge_width"),
    ({"centre": gs.Centre.Custom, "custom_centre_ra": 360.0}, "custom_centre_ra"),
    ({"centre": gs.Centre.Custom, "custom_centre_dec": 91.0}, "custom_centre_dec"),
]


@pytest.mark.parametrize(("kwargs", "name"), CONVERSION_OUT_OF_RANGE)
def test_a_conversion_argument_outside_the_schema_limits_is_rejected(
    client: gs.AsvoClient, httpserver: HTTPServer, kwargs: dict[str, Any], name: str
) -> None:
    """A conversion number outside the schema's limits raises ValueError, and nothing is sent."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match=name):
        client.submit_conversion_job(TEST_OBS_ID, **kwargs)

    assert len(httpserver.log) == requests_before


@pytest.mark.parametrize("offset", [-1, 5401])
def test_a_voltage_offset_outside_the_observation_is_rejected(
    client: gs.AsvoClient, httpserver: HTTPServer, offset: int
) -> None:
    """The offset must be from 0 to 5400 seconds, and nothing is sent otherwise."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="offset"):
        client.submit_voltage_job(TEST_OBS_ID, offset, VOLTAGE_DURATION)

    assert len(httpserver.log) == requests_before


def test_image_from_job_rejects_an_out_of_range_wstack_nwlayers(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """Since schema v1.11 the image-from-job body limits wstack_nwlayers too, and nothing is sent."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="wstack_nwlayers"):
        client.submit_image_from_job(TEST_OBS_ID, SOURCE_JOB_ID, wstack_nwlayers=16)

    assert len(httpserver.log) == requests_before


def test_image_from_job_rejects_a_polarisation_string(client: gs.AsvoClient) -> None:
    """Since schema v1.11 pol is a Polarization for both imaging jobs, not a string."""
    with pytest.raises(TypeError):
        client.submit_image_from_job(TEST_OBS_ID, SOURCE_JOB_ID, pol="XX,YY")  # ty: ignore[invalid-argument-type]


# The endpoint that cancels a job: DELETE <CANCEL_PATH>/<job id>.
CANCEL_PATH = "/api/v2/jobs"
CANCELLED_JOB_ID = 4321


def test_cancel_job_sends_a_delete_and_returns_the_response(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """cancel_job deletes the job and returns the server's reply."""
    httpserver.expect_request(f"{CANCEL_PATH}/{CANCELLED_JOB_ID}", method="DELETE").respond_with_json(
        {"job_id": CANCELLED_JOB_ID, "message": "Job cancelled", "status": "success"}
    )

    response = client.cancel_job(CANCELLED_JOB_ID)

    assert response.job_id == CANCELLED_JOB_ID
    assert response.message == "Job cancelled"
    assert response.status == "success"


def test_cancelling_a_missing_job_raises_asvo_api_error(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """A structured error reply has the server's error code."""
    httpserver.expect_request(f"{CANCEL_PATH}/{CANCELLED_JOB_ID}", method="DELETE").respond_with_json(
        error_response("JOB_NOT_FOUND", "No such job"), status=404
    )

    with pytest.raises(gs.AsvoApiError) as err:
        client.cancel_job(CANCELLED_JOB_ID)

    assert err.value.kind == "ApiError"
    assert err.value.error_code == "JOB_NOT_FOUND"


def test_a_negative_job_id_is_rejected_before_any_request(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """A job ID must fit the library's job ID type, and nothing is sent otherwise."""
    requests_before = len(httpserver.log)

    with pytest.raises(OverflowError):
        client.cancel_job(-1)

    assert len(httpserver.log) == requests_before


def test_conversion_job_sends_no_cable_delay_and_no_rfi(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """The two conversion options reach the request body."""
    submitted(httpserver, CONVERSION_PATH)

    client.submit_conversion_job(TEST_OBS_ID, no_cable_delay=True, no_rfi=True)

    body = body_sent_to(httpserver, CONVERSION_PATH)
    assert body["no_cable_delay"] is True
    assert body["no_rfi"] is True
