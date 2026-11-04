"""Tests for the request-body classes: their checked IDs, `validate()`, the helper functions and `PreprocessJobParams`."""

from collections.abc import Callable
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, TEST_OBS_ID
from .test_submit import (
    CONVERSION_PATH,
    DOWNLOAD_PATH,
    NEW_JOB_ID,
    SOURCE_JOB_ID,
    VOLTAGE_DURATION,
    VOLTAGE_OFFSET,
    body_sent_to,
    submitted,
)

OTHER_OBS_ID = 1065880248
INVALID_OBS_ID = 1
OFFSET_TOO_LARGE = 6000
INVALID_IMAGE_SIZE = 100

# The classes that are made from an Obs ID.
OBS_ID_CLASSES = [gs.DownloadJobParams, gs.ConversionJobParams, gs.ImagingJobParams]


def attributes(params: object) -> dict[str, Any]:
    """The public attributes of a request body, by name.

    Args:
        params: A request-body class.

    Returns:
        The names and values of its attributes.
    """
    return {n: getattr(params, n) for n in dir(params) if not n.startswith("_") and not callable(getattr(params, n))}


@pytest.mark.parametrize("cls", OBS_ID_CLASSES)
def test_the_obs_id_is_an_obs_id(cls: Any) -> None:
    """The attribute is an `ObsId`, whether it was given as an `int` or as an `ObsId`."""
    from_int = cls(TEST_OBS_ID)
    from_obs_id = cls(gs.ObsId(TEST_OBS_ID))

    assert isinstance(from_int.obs_id, gs.ObsId)
    assert isinstance(from_obs_id.obs_id, gs.ObsId)
    assert from_int.obs_id == from_obs_id.obs_id == TEST_OBS_ID


@pytest.mark.parametrize("cls", OBS_ID_CLASSES)
def test_the_obs_id_is_checked_when_it_is_set(cls: Any) -> None:
    """A number that is not an Obs ID raises `ObsIdError` at the assignment, and the old value stays."""
    params = cls(TEST_OBS_ID)

    params.obs_id = OTHER_OBS_ID
    assert params.obs_id == OTHER_OBS_ID
    with pytest.raises(gs.ObsIdErrorWrongNumDigits):
        params.obs_id = INVALID_OBS_ID

    assert params.obs_id == OTHER_OBS_ID


def test_the_source_job_id_is_not_zero() -> None:
    """The library's field is a `NonZeroU64`, so 0 raises when the value is made or set."""
    params = gs.ImageFromJobParams(SOURCE_JOB_ID)

    with pytest.raises(ValueError, match="0 is not a Job ID"):
        gs.ImageFromJobParams(0)
    with pytest.raises(ValueError, match="0 is not a Job ID"):
        params.source_job_id = 0

    assert params.source_job_id == SOURCE_JOB_ID


def test_an_image_from_a_job_has_no_obs_id() -> None:
    """The MWA ASVO takes the Obs ID from the source job, so the class has none."""
    params = gs.ImageFromJobParams(SOURCE_JOB_ID)

    assert not hasattr(params, "obs_id")
    with pytest.raises(TypeError, match="obs_id"):
        gs.ImageFromJobParams(SOURCE_JOB_ID, obs_id=TEST_OBS_ID)  # ty: ignore[unknown-argument]


def test_validate_accepts_the_defaults() -> None:
    """A body that has only what it needs is valid."""
    gs.DownloadJobParams(TEST_OBS_ID).validate()
    gs.DownloadJobParams(TEST_OBS_ID, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION).validate()
    gs.ConversionJobParams(TEST_OBS_ID).validate()
    gs.ImagingJobParams(TEST_OBS_ID).validate()
    gs.ImageFromJobParams(SOURCE_JOB_ID).validate()


def test_validate_refuses_an_offset_outside_the_limits() -> None:
    """The limit is the library's, and it names the value."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="offset"):
        gs.DownloadJobParams(TEST_OBS_ID, offset=OFFSET_TOO_LARGE, duration=VOLTAGE_DURATION).validate()


def test_validate_refuses_a_delivery_that_the_format_does_not_suit() -> None:
    """Acacia delivers a tar file only."""
    params = gs.DownloadJobParams(TEST_OBS_ID, delivery_format=gs.DeliveryFormat.Files)

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="delivery_format"):
        params.validate()


def test_validate_refuses_an_image_size_that_is_not_allowed() -> None:
    """The size must be one of the sizes that the MWA ASVO allows, in both imaging bodies."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="image_size"):
        gs.ImagingJobParams(TEST_OBS_ID, image_size=INVALID_IMAGE_SIZE).validate()
    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="image_size"):
        gs.ImageFromJobParams(SOURCE_JOB_ID, image_size=INVALID_IMAGE_SIZE).validate()


def test_validate_refuses_a_custom_centre_without_a_position() -> None:
    """A custom phase centre needs its RA and Dec."""
    params = gs.ConversionJobParams(TEST_OBS_ID, centre=gs.PhaseCentre.Custom)

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="custom_centre"):
        params.validate()


def test_validate_checks_the_values_as_they_are_now() -> None:
    """A body that was valid, then changed, is checked again."""
    params = gs.DownloadJobParams(TEST_OBS_ID, offset=VOLTAGE_OFFSET, duration=VOLTAGE_DURATION)
    params.validate()

    params.offset = OFFSET_TOO_LARGE

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="offset"):
        params.validate()


def test_download_params_is_the_class_with_its_defaults() -> None:
    """`download_params(obs_id)` is `DownloadJobParams(obs_id)`, with no voltage fields."""
    made = gs.download_params(TEST_OBS_ID)

    assert isinstance(made, gs.DownloadJobParams)
    assert attributes(made) == attributes(gs.DownloadJobParams(TEST_OBS_ID))
    assert made.only_metadata is False
    assert (made.offset, made.duration, made.from_channel, made.to_channel) == (None, None, None, None)


def test_conversion_params_is_the_class_with_its_defaults() -> None:
    """`conversion_params(obs_id)` is `ConversionJobParams(obs_id)`."""
    made = gs.conversion_params(TEST_OBS_ID)

    assert isinstance(made, gs.ConversionJobParams)
    assert attributes(made) == attributes(gs.ConversionJobParams(TEST_OBS_ID))


def test_imaging_params_is_the_class_with_its_defaults() -> None:
    """`imaging_params(obs_id)` is `ImagingJobParams(obs_id)`."""
    made = gs.imaging_params(TEST_OBS_ID)

    assert isinstance(made, gs.ImagingJobParams)
    assert attributes(made) == attributes(gs.ImagingJobParams(TEST_OBS_ID))


def test_image_from_job_params_is_the_class_with_its_defaults() -> None:
    """`image_from_job_params(source_job_id)` is `ImageFromJobParams(source_job_id)`."""
    made = gs.image_from_job_params(SOURCE_JOB_ID)

    assert isinstance(made, gs.ImageFromJobParams)
    assert attributes(made) == attributes(gs.ImageFromJobParams(SOURCE_JOB_ID))


@pytest.mark.parametrize(
    "make",
    [gs.download_params, gs.conversion_params, gs.imaging_params],
)
def test_a_helper_refuses_an_invalid_obs_id(make: Callable[[int], Any]) -> None:
    """The helpers take an `ObsId`, so a number is checked."""
    with pytest.raises(gs.ObsIdErrorWrongNumDigits):
        make(INVALID_OBS_ID)


def test_a_helper_refuses_job_id_zero() -> None:
    """`image_from_job_params` takes a Job ID."""
    with pytest.raises(ValueError, match="0 is not a Job ID"):
        gs.image_from_job_params(0)


def test_a_helper_takes_an_obs_id() -> None:
    """An `ObsId` is accepted, as everywhere."""
    assert gs.download_params(gs.ObsId(TEST_OBS_ID)).obs_id == TEST_OBS_ID


def test_a_preprocess_job_params_holds_its_class() -> None:
    """Each variant has the class that it was made from."""
    conversion = gs.ConversionJobParams(TEST_OBS_ID)
    imaging = gs.ImagingJobParams(TEST_OBS_ID)

    assert gs.PreprocessJobParams.Conversion(conversion).params is conversion
    assert gs.PreprocessJobParams.Imaging(imaging).params is imaging
    assert not hasattr(gs.PreprocessJobParams, "ImageFromJob")


def test_a_preprocess_job_params_can_be_matched() -> None:
    """The variants are classes, so `match` works as it does for a Rust enum."""
    params = gs.PreprocessJobParams.Imaging(gs.ImagingJobParams(TEST_OBS_ID))

    match params:
        case gs.PreprocessJobParams.Imaging(params=inner):
            assert isinstance(inner, gs.ImagingJobParams)
        case _:
            pytest.fail("the variant was not matched")


def test_a_preprocess_job_params_validates_what_it_holds() -> None:
    """`validate` uses the `validate_*_params` function of the class that it holds."""
    gs.PreprocessJobParams.Conversion(gs.ConversionJobParams(TEST_OBS_ID)).validate()
    bad = gs.PreprocessJobParams.Imaging(gs.ImagingJobParams(TEST_OBS_ID, image_size=INVALID_IMAGE_SIZE))

    with pytest.raises(gs.AsvoApiErrorInvalidParameter, match="image_size"):
        bad.validate()


@pytest.fixture
def client(host: str, mock_login: None) -> gs.AsvoClient:
    """A logged-in client of the mock server."""
    return gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))


def test_submit_preprocess_job_takes_a_preprocess_job_params(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """The body that is sent is that of the class inside, the same as when the class is given alone."""
    submitted(httpserver, CONVERSION_PATH)
    conversion = gs.ConversionJobParams(TEST_OBS_ID, avg_freq_res=10.0)

    reply = client.submit_preprocess_job(gs.PreprocessJobParams.Conversion(conversion))
    wrapped = body_sent_to(httpserver, CONVERSION_PATH)
    httpserver.clear_log()
    client.submit_preprocess_job(conversion)
    alone = body_sent_to(httpserver, CONVERSION_PATH)

    assert reply.job_id == NEW_JOB_ID
    assert wrapped == alone
    assert wrapped["obs_id"] == TEST_OBS_ID


def test_submit_preprocess_job_refuses_another_type(client: gs.AsvoClient) -> None:
    """A download body is not a preprocess body."""
    with pytest.raises(TypeError, match="PreprocessJobParams"):
        client.submit_preprocess_job(gs.DownloadJobParams(TEST_OBS_ID))  # ty: ignore[invalid-argument-type]


def test_submit_download_job_sends_a_helper_body(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """A body from a helper is submitted as any other."""
    submitted(httpserver, DOWNLOAD_PATH)

    params = gs.download_params(TEST_OBS_ID)
    params.offset = VOLTAGE_OFFSET
    params.duration = VOLTAGE_DURATION
    client.submit_download_job(params)

    body = body_sent_to(httpserver, DOWNLOAD_PATH)
    assert body["obs_id"] == TEST_OBS_ID
    assert body["offset"] == VOLTAGE_OFFSET
    assert body["duration"] == VOLTAGE_DURATION
