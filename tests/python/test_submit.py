"""Tests for the submit methods, against a local mock MWA ASVO.

Each test checks the request body the mock receives, because that is what the MWA ASVO acts on. A keyword
argument that is left out must be absent from what the module sets, so that the schema default applies: those
tests compare with the default the schema gives, never with a value the module chose.
"""

import json
from collections.abc import Callable
from functools import partial
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import IMAGE_FROM_JOB, IMAGING_JOB, TEST_API_KEY, TEST_OBS_ID, TEST_OUTPUT_MODE, error_response
from .test_client import SCHEMA_PATH

# The endpoint of each job type.
DOWNLOAD_PATH = "/api/v2/jobs/download"
CONVERSION_PATH = "/api/v2/jobs/preprocess"
IMAGING_PATH = "/api/v2/jobs/preprocess"
IMAGE_FROM_JOB_PATH = "/api/v2/jobs/image_from_job"
VOLTAGE_PATH = "/api/v2/jobs/download"
BEAMFORMER_PATH = "/api/v2/jobs/download"

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
NMITER = 7
WSTACK_LAYERS = 64
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
def client(host: str, mock_login: None) -> gs.MwaAsvoClient:
    """A client that has logged in to the mock.

    Args:
        host: The mock's base URL.
        mock_login: Serves the login.

    Returns:
        The client.
    """
    return gs.MwaAsvoClient(gs.MwaAsvoClientConfig(host, TEST_API_KEY))


def assert_response(response: gs.JobSubmittedResponse) -> None:
    """Check that ``response`` is the one that ``submitted`` serves.

    Args:
        response: What the submit method returned.
    """
    assert response.job_id == NEW_JOB_ID
    assert response.message == NEW_JOB_MESSAGE
    assert response.status == gs.Status.Success
    assert str(response.status) == "success"


def submit(client: gs.MwaAsvoClient, params: Any) -> gs.JobSubmittedResponse:
    """Submit ``params`` with the submit method for its class.

    Args:
        client: A logged-in client.
        params: A ``DownloadJobParams`` (download endpoint), a conversion or imaging class (preprocess endpoint), or
            an ``ImageFromJobParams`` (image-from-job endpoint).

    Returns:
        The server's reply.
    """
    if isinstance(params, gs.DownloadJobParams):
        return client.submit_download_job(params)
    if isinstance(params, gs.ImageFromJobParams):
        return client.submit_image_from_job(params)
    return client.submit_preprocess_job(params)


# The download jobs that have no voltage fields, with their endpoint.
DOWNLOAD_JOBS = [
    pytest.param(gs.DownloadJobParams, DOWNLOAD_PATH, id="download"),
    pytest.param(partial(gs.DownloadJobParams, only_metadata=True), DOWNLOAD_PATH, id="meta"),
]


@pytest.mark.parametrize(("job", "path"), DOWNLOAD_JOBS)
def test_download_jobs_send_the_arguments_and_return_the_response(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, job: Callable[..., Any], path: str
) -> None:
    """Each argument reaches the request body, and the reply is a JobSubmittedResponse."""
    submitted(httpserver, path)

    response = client.submit_download_job(
        job(
            TEST_OBS_ID,
            delivery=gs.DeliveryDestination.Dug,
            delivery_format=gs.DeliveryFormat.Files,
            allow_resubmit=True,
        )
    )

    assert_response(response)
    body = body_sent_to(httpserver, path)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["delivery"] == "dug"
    assert body["delivery_format"] == "files"
    assert body["allow_resubmit"] is True


@pytest.mark.parametrize(("job", "path"), DOWNLOAD_JOBS)
def test_download_jobs_use_the_schema_defaults_when_arguments_are_left_out(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, job: Callable[..., Any], path: str
) -> None:
    """With no optional arguments, the body has the schema's defaults, and none of the voltage fields."""
    submitted(httpserver, path)

    client.submit_download_job(job(TEST_OBS_ID))

    body = body_sent_to(httpserver, path)
    assert body["delivery"] == "acacia"
    assert body["delivery_format"] == "tar"
    assert body["allow_resubmit"] is False
    for name in ("offset", "duration", "from_channel", "to_channel"):
        assert name not in body


def test_vis_and_meta_jobs_differ_only_in_only_metadata(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """Both use the download endpoint."""
    submitted(httpserver, DOWNLOAD_PATH)

    client.submit_download_job(gs.DownloadJobParams(TEST_OBS_ID))
    client.submit_download_job(gs.DownloadJobParams(TEST_OBS_ID, only_metadata=True))

    # The schema gives `only_metadata` no default, so a visibility job leaves it out.
    only_metadata = [
        json.loads(request.get_data()).get("only_metadata")
        for request, _ in httpserver.log
        if request.path == DOWNLOAD_PATH
    ]
    assert only_metadata == [None, True]


def test_conversion_job_sends_the_arguments(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """Every conversion argument reaches the request body under its OpenAPI name."""
    submitted(httpserver, CONVERSION_PATH)

    response = client.submit_preprocess_job(
        gs.ConversionJobParams(
            TEST_OBS_ID,
            delivery=gs.DeliveryDestination.Scratch,
            delivery_format=gs.DeliveryFormat.Files,
            output=gs.OutputFormat.Uvfits,
            avg_freq_res=FREQ_RES,
            avg_time_res=FREQ_RES,
            flag_edge_width=FREQ_RES,
            apply_di_cal=True,
            centre=gs.PhaseCentre.Custom,
            custom_centre_ra=CUSTOM_RA,
            custom_centre_dec=CUSTOM_DEC,
            no_apply_amps=True,
            no_digital_gains=True,
            no_flag_dc=True,
            no_geometric_delay=True,
            no_passband_gains=True,
            allow_resubmit=True,
        )
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
        "no_geometric_delay": True,
        "no_passband_gains": True,
        "allow_resubmit": True,
        # Not given, so the schema defaults apply.
        "no_cable_delay": False,
        "no_rfi": False,
    }


def test_conversion_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """The module adds no defaults of its own; the schema's apply."""
    submitted(httpserver, CONVERSION_PATH)

    client.submit_preprocess_job(gs.ConversionJobParams(TEST_OBS_ID))

    body = body_sent_to(httpserver, CONVERSION_PATH)
    assert body["output"] == "ms"
    assert body["centre"] == "phase"
    assert body["apply_di_cal"] is False
    assert "custom_centre_ra" not in body
    assert "custom_centre_dec" not in body


def test_imaging_job_sends_the_arguments(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """Enum and numeric imaging arguments reach the request body."""
    submitted(httpserver, IMAGING_PATH)

    response = client.submit_preprocess_job(
        IMAGING_JOB(
            TEST_OBS_ID,
            apply_di_cal=False,
            centre=gs.PhaseCentre.Pointing,
            image_size=VALID_IMAGE_SIZE,
            nmiter=NMITER,
            output_mode=gs.ImageOutputMode.AllFits,
            polarization=gs.Polarization.Xx,
            weighting=gs.ImageWeighting.Natural,
            uv_max=FREQ_RES,
            wstack_nwlayers=WSTACK_LAYERS,
            join_polarizations=True,
            allow_resubmit=True,
        )
    )

    assert_response(response)
    body = body_sent_to(httpserver, IMAGING_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["apply_di_cal"] is False
    assert body["centre"] == "pointing"
    assert body["image_size"] == VALID_IMAGE_SIZE
    assert body["nmiter"] == NMITER
    assert body["output_mode"] == "all_fits"
    assert body["polarization"] == "xx"
    assert body["weighting"] == "natural"
    assert body["uv_max"] == FREQ_RES
    assert body["wstack_nwlayers"] == WSTACK_LAYERS
    assert body["join_polarizations"] is True
    assert body["allow_resubmit"] is True


def test_imaging_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """Optional fields with no schema default are not sent."""
    submitted(httpserver, IMAGING_PATH)

    client.submit_preprocess_job(IMAGING_JOB(TEST_OBS_ID))

    body = body_sent_to(httpserver, IMAGING_PATH)
    assert body["polarization"] == "i"
    assert body["output_mode"] == "fits"
    assert body["weighting"] == "briggs"
    assert body["apply_di_cal"] is True
    for name in ("custom_centre_ra", "custom_centre_dec", "uv_max", "wstack_nwlayers", "image_size", "pixel_scale"):
        assert name not in body


IMAGING_SWITCHES = (
    "no_digital_gains",
    "no_flag_dc",
    "no_geometric_delay",
    "no_passband_gains",
    "no_cable_delay",
    "no_rfi",
)


# Each kind of job: a function that makes its request body, its endpoint, the arguments of one call, and the schema
# that defines its request body.
BODY_CASES: list[tuple[str, Callable[..., Any], str, tuple[Any, ...], dict[str, Any], str]] = [
    ("vis", gs.DownloadJobParams, DOWNLOAD_PATH, (TEST_OBS_ID,), {"allow_resubmit": True}, "DownloadJobParams"),
    (
        "meta",
        gs.DownloadJobParams,
        DOWNLOAD_PATH,
        (TEST_OBS_ID,),
        {"only_metadata": True},
        "DownloadJobParams",
    ),
    (
        "voltage",
        gs.DownloadJobParams,
        VOLTAGE_PATH,
        (TEST_OBS_ID,),
        {"offset": 10, "duration": 32, "from_channel": 3},
        "DownloadJobParams",
    ),
    (
        "conversion",
        gs.ConversionJobParams,
        CONVERSION_PATH,
        (TEST_OBS_ID,),
        {"avg_freq_res": 10.0},
        "ConversionJobParams",
    ),
    ("imaging", IMAGING_JOB, IMAGING_PATH, (TEST_OBS_ID,), {"image_size": 1024}, "ImagingJobParams"),
    ("image_from_job", IMAGE_FROM_JOB, IMAGE_FROM_JOB_PATH, (555,), {"nmiter": 7}, "ImageFromJobParams"),
]
BODY_CASE_IDS = [name for name, *_ in BODY_CASES]


@pytest.mark.parametrize(("name", "job", "path", "args", "kwargs", "schema_name"), BODY_CASES, ids=BODY_CASE_IDS)
def test_every_field_of_a_body_is_in_the_schema(
    client: gs.MwaAsvoClient,
    httpserver: HTTPServer,
    name: str,
    job: Callable[..., Any],
    path: str,
    args: tuple[Any, ...],
    kwargs: dict[str, Any],
    schema_name: str,
) -> None:
    """Only parameters that the API defines are sent: no `flags`, and nothing else the schema does not have."""
    submitted(httpserver, path)
    properties = json.loads(SCHEMA_PATH.read_text())["definitions"][schema_name]["properties"]

    submit(client, job(*args, **kwargs))

    body = body_sent_to(httpserver, path)
    assert not [key for key in body if key not in properties]
    assert "flags" not in body


def test_imaging_job_sends_the_six_correction_and_flagging_switches(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """Each switch reaches the body when it is given, and is ``False`` (the schema default) when it is not."""
    submitted(httpserver, IMAGING_PATH)

    client.submit_preprocess_job(IMAGING_JOB(TEST_OBS_ID))
    default_body = body_sent_to(httpserver, IMAGING_PATH)
    client.submit_preprocess_job(
        IMAGING_JOB(
            TEST_OBS_ID,
            no_digital_gains=True,
            no_flag_dc=True,
            no_geometric_delay=True,
            no_passband_gains=True,
            no_cable_delay=True,
            no_rfi=True,
        )
    )
    given_body = [json.loads(request.get_data()) for request, _ in httpserver.log if request.path == IMAGING_PATH][-1]

    for name in IMAGING_SWITCHES:
        assert default_body[name] is False, name
        assert given_body[name] is True, name


def test_image_from_job_sends_the_source_job_and_the_arguments(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """The source job ID and the arguments reach the request body."""
    submitted(httpserver, IMAGE_FROM_JOB_PATH)

    response = client.submit_image_from_job(
        IMAGE_FROM_JOB(
            SOURCE_JOB_ID,
            polarization=gs.Polarization.Yy,
            weighting=gs.ImageWeighting.Uniform,
            output_mode=gs.ImageOutputMode.AllFiles,
            image_size=VALID_IMAGE_SIZE,
            nmiter=NMITER,
            allow_resubmit=True,
        )
    )

    assert_response(response)
    body = body_sent_to(httpserver, IMAGE_FROM_JOB_PATH)
    # The body has no Obs ID: the MWA ASVO takes it from the source job.
    assert "obs_id" not in body
    assert body["source_job_id"] == SOURCE_JOB_ID
    assert body["polarization"] == "yy"
    assert body["weighting"] == "uniform"
    assert body["output_mode"] == "all_files"
    assert body["image_size"] == VALID_IMAGE_SIZE
    assert body["nmiter"] == NMITER
    assert body["allow_resubmit"] is True


def test_image_from_job_uses_the_schema_defaults_when_arguments_are_left_out(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """The image-from-job default for polarization is the Polarization default, as for imaging."""
    submitted(httpserver, IMAGE_FROM_JOB_PATH)

    client.submit_image_from_job(IMAGE_FROM_JOB(SOURCE_JOB_ID))

    body = body_sent_to(httpserver, IMAGE_FROM_JOB_PATH)
    assert body["polarization"] == "i"
    for name in ("image_size", "pixel_scale"):
        assert name not in body
    assert body["weighting"] == "briggs"


def test_voltage_job_sends_the_arguments_and_no_channel_range(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """A voltage job has an offset and a duration, and the schema's delivery; the channel bounds are sent as given."""
    submitted(httpserver, VOLTAGE_PATH)

    response = client.submit_download_job(
        gs.DownloadJobParams(
            TEST_OBS_ID,
            offset=VOLTAGE_OFFSET,
            duration=VOLTAGE_DURATION,
            from_channel=FROM_CHANNEL,
            to_channel=TO_CHANNEL,
            allow_resubmit=True,
        )
    )

    assert_response(response)
    body = body_sent_to(httpserver, VOLTAGE_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["offset"] == VOLTAGE_OFFSET
    assert body["duration"] == VOLTAGE_DURATION
    assert body["delivery"] == "acacia"
    assert body["delivery_format"] == "tar"
    assert body["from_channel"] == FROM_CHANNEL
    assert body["to_channel"] == TO_CHANNEL
    assert "channel_range" not in body
    assert body["allow_resubmit"] is True


def test_voltage_job_without_channels_sends_no_channel(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """With no channel bound, no channel is sent."""
    submitted(httpserver, VOLTAGE_PATH)

    client.submit_download_job(gs.DownloadJobParams(TEST_OBS_ID, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION))

    body = body_sent_to(httpserver, VOLTAGE_PATH)
    assert "channel_range" not in body
    assert "from_channel" not in body
    assert "to_channel" not in body


def test_a_server_error_raises_mwa_asvo_api_error(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """An error reply to a submission has the server's error fields."""
    httpserver.expect_request(DOWNLOAD_PATH, method="POST").respond_with_json(
        error_response("JOB_ALREADY_SUBMITTED", "An identical job exists"), status=409
    )

    with pytest.raises(gs.MwaAsvoApiError) as err:
        client.submit_download_job(gs.DownloadJobParams(TEST_OBS_ID))

    assert err.value.kind == "Server"
    assert err.value.error_code == "JOB_ALREADY_SUBMITTED"


# Every kind of request body, made with only what it needs, so that one call shows an argument problem.
ALL_JOBS: list[tuple[str, Callable[[int], Any]]] = [
    ("download", lambda obs_id: gs.DownloadJobParams(obs_id)),
    (
        "voltage",
        lambda obs_id: gs.DownloadJobParams(obs_id, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION),
    ),
    ("conversion", lambda obs_id: gs.ConversionJobParams(obs_id)),
    ("imaging", lambda obs_id: IMAGING_JOB(obs_id)),
]


@pytest.mark.parametrize(("name", "make"), ALL_JOBS, ids=[name for name, _ in ALL_JOBS])
def test_an_invalid_obs_id_is_rejected_when_the_class_is_made(name: str, make: Callable[[int], Any]) -> None:
    """Every request body checks its Obs ID as the library's `ObsId` does, so there is nothing to submit."""
    with pytest.raises(gs.ObsIdErrorWrongNumDigits, match="not a valid MWA Obs ID") as excinfo:
        make(INVALID_OBS_ID)

    assert excinfo.value.value == INVALID_OBS_ID, name


# The two imaging classes, with the argument that names the job: the Obs ID, or the source job.
IMAGING_JOBS = [
    pytest.param(IMAGING_JOB, TEST_OBS_ID, IMAGING_PATH, id="imaging"),
    pytest.param(IMAGE_FROM_JOB, SOURCE_JOB_ID, IMAGE_FROM_JOB_PATH, id="image_from_job"),
]


@pytest.mark.parametrize(("job", "first", "path"), IMAGING_JOBS)
def test_an_invalid_image_size_is_rejected(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, job: Callable[..., Any], first: int, path: str
) -> None:
    """An image size the API does not accept raises ValueError at the submission, and nothing is sent."""
    params = job(first, image_size=INVALID_IMAGE_SIZE)
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="image_size"):
        submit(client, params)

    assert len(httpserver.log) == requests_before


@pytest.mark.parametrize(("job", "first", "path"), IMAGING_JOBS)
def test_a_zero_nmiter_is_rejected(client: gs.MwaAsvoClient, job: Callable[..., Any], first: int, path: str) -> None:
    """nmiter must be greater than zero."""
    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="nmiter"):
        submit(client, job(first, nmiter=0))


def test_a_zero_source_job_id_is_rejected(client: gs.MwaAsvoClient) -> None:
    """source_job_id must be greater than zero.

    The match is the library's message: the argument name is only in the note that PyO3 adds on Python 3.11 and later.
    """
    with pytest.raises(ValueError, match="0 is not a Job ID"):
        client.submit_image_from_job(IMAGE_FROM_JOB(0))


def test_a_channel_number_out_of_range_is_rejected() -> None:
    """A voltage channel number must fit in one byte."""
    with pytest.raises(OverflowError):
        gs.DownloadJobParams(TEST_OBS_ID, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION, from_channel=256)


def test_optional_arguments_must_be_given_by_keyword() -> None:
    """Optional arguments are keyword-only."""
    with pytest.raises(TypeError):
        gs.DownloadJobParams(TEST_OBS_ID, gs.DeliveryDestination.Acacia)  # ty: ignore[too-many-positional-arguments]


def test_a_delivery_of_the_wrong_type_is_rejected() -> None:
    """The delivery argument is an enum member, not a string."""
    with pytest.raises(TypeError):
        gs.DownloadJobParams(TEST_OBS_ID, delivery="acacia")  # ty: ignore[invalid-argument-type]


def test_the_classes_are_plain_classes_with_attributes_that_can_be_changed(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """Each argument is an attribute: it is read as given, and a change reaches the request body."""
    submitted(httpserver, CONVERSION_PATH)
    params = gs.ConversionJobParams(TEST_OBS_ID, avg_freq_res=FREQ_RES)

    assert params.obs_id == TEST_OBS_ID
    assert params.avg_freq_res == FREQ_RES
    assert params.delivery == gs.DeliveryDestination.Acacia
    assert params.custom_centre_ra is None

    params.avg_freq_res = None
    params.no_rfi = True
    params.delivery_format = gs.DeliveryFormat.Files
    params.delivery = gs.DeliveryDestination.Scratch
    client.submit_preprocess_job(params)

    body = body_sent_to(httpserver, CONVERSION_PATH)
    assert body["avg_freq_res"] is None
    assert body["no_rfi"] is True
    assert body["delivery"] == "scratch"
    assert body["delivery_format"] == "files"


def test_an_invalid_value_is_checked_at_the_submission_not_when_the_class_is_made(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """The classes check nothing: a bad value raises ValueError from the submit method, and nothing is sent."""
    params = IMAGING_JOB(TEST_OBS_ID, mgain=1.0)
    params.mgain = 1.5
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="mgain"):
        client.submit_preprocess_job(params)

    assert len(httpserver.log) == requests_before


def test_the_same_class_can_be_submitted_again_after_a_change(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """A submission does not use up the object: it is a request body, and can be sent again."""
    submitted(httpserver, DOWNLOAD_PATH)
    params = gs.DownloadJobParams(TEST_OBS_ID)

    client.submit_download_job(params)
    params.allow_resubmit = True
    client.submit_download_job(params)

    first, second = [json.loads(r.get_data()) for r, _ in httpserver.log if r.path == DOWNLOAD_PATH]
    assert first["allow_resubmit"] is False
    assert second["allow_resubmit"] is True


@pytest.mark.parametrize(
    "wrong",
    [None, TEST_OBS_ID, "ConversionJobParams", gs.DownloadJobParams(TEST_OBS_ID), IMAGE_FROM_JOB(1)],
)
def test_the_preprocess_method_takes_only_the_preprocess_classes(client: gs.MwaAsvoClient, wrong: Any) -> None:
    """Any other object raises TypeError, which names the classes (a download or image-from-job body is not one)."""
    with pytest.raises(TypeError, match="PreprocessJobParams, a ConversionJobParams or an ImagingJobParams"):
        client.submit_preprocess_job(wrong)


@pytest.mark.parametrize("wrong", [None, TEST_OBS_ID, gs.ConversionJobParams(TEST_OBS_ID)])
def test_the_download_method_takes_only_a_download_body(client: gs.MwaAsvoClient, wrong: Any) -> None:
    """Any other object raises TypeError, which names the download class."""
    with pytest.raises(TypeError, match="DownloadJobParams"):
        client.submit_download_job(wrong)


def test_the_new_enums_use_the_api_values_as_their_text() -> None:
    """str() of a member is the value the API uses."""
    assert str(gs.DeliveryFormat.Tar) == "tar"
    assert str(gs.OutputFormat.Uvfits) == "uvfits"
    assert str(gs.PhaseCentre.Custom) == "custom"
    assert str(gs.ImageOutputMode.AllFits) == "all_fits"
    assert str(gs.ImageWeighting.Briggs) == "briggs"
    assert str(gs.Polarization.Xxyy) == "xxyy"
    assert gs.OutputFormat.Ms != gs.OutputFormat.Uvfits


def test_the_response_has_a_readable_repr(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """repr() names the class and shows the job ID."""
    submitted(httpserver, DOWNLOAD_PATH)

    response = client.submit_download_job(gs.DownloadJobParams(TEST_OBS_ID))

    assert repr(response).startswith("JobSubmittedResponse(")
    assert str(NEW_JOB_ID) in repr(response)


# Arguments that are outside the schema's limits: (the class, keyword arguments, the name the error must give).
OUT_OF_RANGE = [
    (IMAGING_JOB, {"mgain": 1.5}, "mgain"),
    (IMAGING_JOB, {"mgain": 0.05}, "mgain"),
    (IMAGING_JOB, {"nmiter": 501}, "nmiter"),
    (IMAGING_JOB, {"auto_mask": 1}, "auto_mask"),
    (IMAGING_JOB, {"auto_threshold": 6.0}, "auto_threshold"),
    (IMAGING_JOB, {"pixel_scale": 9.0}, "pixel_scale"),
    (IMAGING_JOB, {"robust": -2.5}, "robust"),
    (IMAGING_JOB, {"clean_iterations": 1_000_001}, "clean_iterations"),
    (IMAGING_JOB, {"abs_threshold": 11.0}, "abs_threshold"),
    (IMAGING_JOB, {"uv_max": 0.5}, "uv_max"),
    (IMAGING_JOB, {"uv_min": 101.0}, "uv_min"),
    (IMAGING_JOB, {"avg_freq_res": 1281.0}, "avg_freq_res"),
    (IMAGING_JOB, {"avg_time_res": -1.0}, "avg_time_res"),
    (IMAGING_JOB, {"flag_edge_width": 641.0}, "flag_edge_width"),
    (
        IMAGING_JOB,
        {"centre": gs.PhaseCentre.Custom, "custom_centre_ra": 360.0},
        "custom_centre_ra",
    ),
    (
        IMAGING_JOB,
        {"centre": gs.PhaseCentre.Custom, "custom_centre_dec": -91.0},
        "custom_centre_dec",
    ),
    (IMAGING_JOB, {"mgain": float("nan")}, "mgain"),
    (IMAGING_JOB, {"wstack_nwlayers": 16}, "wstack_nwlayers"),
    (IMAGING_JOB, {"wstack_nwlayers": 513}, "wstack_nwlayers"),
    (IMAGE_FROM_JOB, {"mgain": 1.5}, "mgain"),
    (IMAGE_FROM_JOB, {"nmiter": 501}, "nmiter"),
    (IMAGE_FROM_JOB, {"pixel_scale": 121.0}, "pixel_scale"),
]


def imaging_args(job: Callable[..., Any]) -> tuple[int, ...]:
    """The positional arguments of an imaging class.

    Args:
        job: The class.

    Returns:
        The obsid, and the source job ID for ``ImageFromJobParams``.
    """
    return (SOURCE_JOB_ID,) if job is IMAGE_FROM_JOB else (TEST_OBS_ID,)


@pytest.mark.parametrize(("job", "kwargs", "name"), OUT_OF_RANGE)
def test_an_argument_outside_the_schema_limits_is_rejected_before_any_request(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, job: Callable[..., Any], kwargs: dict[str, Any], name: str
) -> None:
    """A number outside the schema's limits raises ValueError that names the argument, and nothing is sent."""
    params = job(*imaging_args(job), **kwargs)
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match=name):
        submit(client, params)

    assert len(httpserver.log) == requests_before


def test_the_error_message_gives_the_limits(client: gs.MwaAsvoClient) -> None:
    """The message is the library's: the same text the CLI and the Rust client give."""
    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter) as err:
        client.submit_preprocess_job(IMAGING_JOB(TEST_OBS_ID, mgain=1.5))

    assert str(err.value) == "Invalid mgain: must be between 0.1 and 1 (got 1.5)"


def test_the_supported_image_sizes_are_listed_in_the_error(client: gs.MwaAsvoClient) -> None:
    """An unsupported image size is reported with the sizes that the MWA ASVO accepts."""
    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="512, 1024, 2048, 3072, 4096, 8192"):
        client.submit_preprocess_job(IMAGING_JOB(TEST_OBS_ID, image_size=INVALID_IMAGE_SIZE))


@pytest.mark.parametrize(("job", "first", "path"), IMAGING_JOBS)
def test_values_at_the_limits_are_accepted_and_sent(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, job: Callable[..., Any], first: int, path: str
) -> None:
    """The ends of each range are valid."""
    submitted(httpserver, path)

    submit(client, job(first, mgain=1.0, nmiter=500, pixel_scale=10.0, robust=-2.0, uv_min=100.0, auto_mask=512))

    body = body_sent_to(httpserver, path)
    assert body["mgain"] == 1.0
    assert body["nmiter"] == 500
    assert body["pixel_scale"] == 10.0
    assert body["robust"] == -2.0
    assert body["uv_min"] == 100.0
    assert body["auto_mask"] == 512


# Conversion arguments that are outside the schema's limits: (keyword arguments, the name the error must give).
CONVERSION_OUT_OF_RANGE = [
    ({"avg_freq_res": 1281.0}, "avg_freq_res"),
    ({"avg_freq_res": -1.0}, "avg_freq_res"),
    ({"avg_time_res": -1.0}, "avg_time_res"),
    ({"flag_edge_width": 641.0}, "flag_edge_width"),
    ({"centre": gs.PhaseCentre.Custom, "custom_centre_ra": 360.0}, "custom_centre_ra"),
    ({"centre": gs.PhaseCentre.Custom, "custom_centre_dec": 91.0}, "custom_centre_dec"),
]


@pytest.mark.parametrize(("kwargs", "name"), CONVERSION_OUT_OF_RANGE)
def test_a_conversion_argument_outside_the_schema_limits_is_rejected(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, kwargs: dict[str, Any], name: str
) -> None:
    """A conversion number outside the schema's limits raises ValueError, and nothing is sent."""
    params = gs.ConversionJobParams(TEST_OBS_ID, **kwargs)
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match=name):
        client.submit_preprocess_job(params)

    assert len(httpserver.log) == requests_before


@pytest.mark.parametrize("offset", [-1, 7201])
def test_a_voltage_offset_outside_the_observation_is_rejected(
    client: gs.MwaAsvoClient, httpserver: HTTPServer, offset: int
) -> None:
    """The offset must be from 0 to 7200 seconds, and nothing is sent otherwise."""
    params = gs.DownloadJobParams(TEST_OBS_ID, offset=offset, duration=VOLTAGE_DURATION)
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="offset"):
        client.submit_download_job(params)

    assert len(httpserver.log) == requests_before


def test_image_from_job_rejects_an_out_of_range_wstack_nwlayers(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """Since schema v1.11 the image-from-job body limits wstack_nwlayers too, and nothing is sent."""
    params = IMAGE_FROM_JOB(SOURCE_JOB_ID, wstack_nwlayers=16)
    requests_before = len(httpserver.log)

    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter, match="wstack_nwlayers"):
        client.submit_image_from_job(params)

    assert len(httpserver.log) == requests_before


def test_the_imaging_classes_require_an_output_mode() -> None:
    """The schema requires `output_mode` and gives it no default."""
    with pytest.raises(TypeError, match="output_mode"):
        gs.ImagingJobParams(TEST_OBS_ID)  # ty: ignore[missing-argument]
    with pytest.raises(TypeError, match="output_mode"):
        gs.ImageFromJobParams(SOURCE_JOB_ID)  # ty: ignore[missing-argument]


def test_image_from_job_rejects_a_polarisation_string() -> None:
    """polarization is a Polarization for both imaging jobs, not a string."""
    with pytest.raises(TypeError):
        gs.ImageFromJobParams(
            SOURCE_JOB_ID,
            output_mode=TEST_OUTPUT_MODE,
            polarization="xxyy",  # ty: ignore[invalid-argument-type]
        )


# The endpoint that cancels a job: DELETE <CANCEL_PATH>/<job id>.
CANCEL_PATH = "/api/v2/jobs"
CANCELLED_JOB_ID = 4321


def test_cancel_job_sends_a_delete_and_returns_the_response(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """cancel_job deletes the job and returns the server's reply."""
    httpserver.expect_request(f"{CANCEL_PATH}/{CANCELLED_JOB_ID}", method="DELETE").respond_with_json(
        {"job_id": CANCELLED_JOB_ID, "message": "Job cancelled", "status": "success"}
    )

    response = client.cancel_job(CANCELLED_JOB_ID)

    assert isinstance(response, gs.JobCancelledResponse)
    assert response.job_id == CANCELLED_JOB_ID
    assert response.message == "Job cancelled"
    assert response.status == gs.Status.Success
    assert repr(response).startswith("JobCancelledResponse(")


def test_cancelling_a_missing_job_raises_mwa_asvo_api_error(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """A structured error reply has the server's error code."""
    httpserver.expect_request(f"{CANCEL_PATH}/{CANCELLED_JOB_ID}", method="DELETE").respond_with_json(
        error_response("JOB_NOT_FOUND", "No such job"), status=404
    )

    with pytest.raises(gs.MwaAsvoApiError) as err:
        client.cancel_job(CANCELLED_JOB_ID)

    assert err.value.kind == "Server"
    assert err.value.error_code == "JOB_NOT_FOUND"


def test_a_negative_job_id_is_rejected_before_any_request(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """A job ID must fit the library's job ID type, and nothing is sent otherwise."""
    requests_before = len(httpserver.log)

    with pytest.raises(OverflowError):
        client.cancel_job(-1)

    assert len(httpserver.log) == requests_before


def test_job_id_zero_is_rejected_before_any_request(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """0 is not a job ID (job IDs start at 1), and nothing is sent."""
    requests_before = len(httpserver.log)

    with pytest.raises(ValueError, match="0 is not a Job ID"):
        client.cancel_job(0)

    assert len(httpserver.log) == requests_before


def test_conversion_job_sends_no_cable_delay_and_no_rfi(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """The two conversion options reach the request body."""
    submitted(httpserver, CONVERSION_PATH)

    client.submit_preprocess_job(gs.ConversionJobParams(TEST_OBS_ID, no_cable_delay=True, no_rfi=True))

    body = body_sent_to(httpserver, CONVERSION_PATH)
    assert body["no_cable_delay"] is True
    assert body["no_rfi"] is True


@pytest.mark.parametrize("name", ["clean_threshold", "nwlayers", "pol"])
@pytest.mark.parametrize(("job", "first"), [(IMAGING_JOB, TEST_OBS_ID), (IMAGE_FROM_JOB, 1)])
def test_the_imaging_classes_have_no_old_arguments(job: Callable[..., Any], first: int, name: str) -> None:
    """The schema no longer has `clean_threshold` and `nwlayers`, and `pol` is now `polarization`."""
    with pytest.raises(TypeError, match=name):
        job(first, **{name: 1})
    assert not hasattr(job(first), name)
