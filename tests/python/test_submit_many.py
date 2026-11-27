"""Tests for MwaAsvoClient.submit_many, against a local mock MWA ASVO.

The mock refuses the jobs of one Obs ID and accepts the others, so that a test shows that a failure does not stop
the other submissions.
"""

import json

import pytest
from pytest_httpserver import HTTPServer
from werkzeug import Request, Response

import mwa_giant_squid as gs

from .conftest import IMAGING_JOB, TEST_OBS_ID, TEST_OUTPUT_MODE, error_response

DOWNLOAD_PATH = "/api/v2/jobs/download"
PREPROCESS_PATH = "/api/v2/jobs/preprocess"

# An Obs ID that the mock refuses, with this error code, and one that it accepts.
REFUSED_OBS_ID = TEST_OBS_ID
REFUSED_ERROR_CODE = "VALID_OBS_NOT_FOUND"
ACCEPTED_OBS_ID = 1065880136

# The Job ID that the mock gives each accepted job.
NEW_JOB_ID = 778

# A value above the schema's maximum of `avg_freq_res` (1280).
TOO_LARGE_FREQ_RES = 5000.0

# What a JSON error reply has as its HTTP status.
HTTP_BAD_REQUEST = 400


def serve_submissions(httpserver: HTTPServer, path: str) -> None:
    """Serve submissions at ``path``: refuse the job of ``REFUSED_OBS_ID`` and accept the others.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.
    """

    def handler(request: Request) -> Response:
        body = json.loads(request.get_data())
        if body["obs_id"] == REFUSED_OBS_ID:
            reply = error_response(REFUSED_ERROR_CODE, "No such observation")
            return Response(json.dumps(reply), status=HTTP_BAD_REQUEST, content_type="application/json")
        reply = {"job_id": NEW_JOB_ID, "message": "Job submitted", "status": "success"}
        return Response(json.dumps(reply), content_type="application/json")

    httpserver.expect_request(path, method="POST").respond_with_handler(handler)


def obs_ids_sent_to(httpserver: HTTPServer, path: str) -> list[int]:
    """The Obs IDs of the bodies that the mock received at ``path``, in order.

    Args:
        httpserver: The pytest-httpserver server.
        path: The endpoint.

    Returns:
        The Obs IDs.
    """
    return [json.loads(request.get_data())["obs_id"] for request, _ in httpserver.log if request.path == path]


def test_each_obs_id_is_submitted_and_a_failure_does_not_stop_the_others(
    client: gs.MwaAsvoClient, httpserver: HTTPServer
) -> None:
    """One job for each Obs ID, in order, with one result each."""
    serve_submissions(httpserver, DOWNLOAD_PATH)
    params = gs.DownloadJobParams(ACCEPTED_OBS_ID, only_metadata=True)

    results = client.submit_many([REFUSED_OBS_ID, ACCEPTED_OBS_ID, gs.ObsId(REFUSED_OBS_ID)], params)

    assert obs_ids_sent_to(httpserver, DOWNLOAD_PATH) == [REFUSED_OBS_ID, ACCEPTED_OBS_ID, REFUSED_OBS_ID]
    assert [result.obs_id for result in results] == [REFUSED_OBS_ID, ACCEPTED_OBS_ID, REFUSED_OBS_ID]
    assert all(isinstance(result.obs_id, gs.ObsId) for result in results)

    refused, accepted, _ = results
    assert refused.response is None
    assert isinstance(refused.error, gs.MwaAsvoApiErrorServer)
    assert refused.error.error_code == REFUSED_ERROR_CODE
    assert accepted.error is None
    assert accepted.response is not None
    assert accepted.response.job_id == NEW_JOB_ID


def test_each_body_is_the_params_with_the_obs_id_of_the_job(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """The other fields of ``params`` are sent unchanged."""
    serve_submissions(httpserver, DOWNLOAD_PATH)

    client.submit_many([ACCEPTED_OBS_ID], gs.DownloadJobParams(REFUSED_OBS_ID, only_metadata=True))

    ((request, _),) = httpserver.log[-1:]
    body = json.loads(request.get_data())
    assert body["obs_id"] == ACCEPTED_OBS_ID
    assert body["only_metadata"] is True


@pytest.mark.parametrize(
    "params",
    [
        gs.ConversionJobParams(ACCEPTED_OBS_ID),
        IMAGING_JOB(ACCEPTED_OBS_ID),
        gs.PreprocessJobParams.Conversion(gs.ConversionJobParams(ACCEPTED_OBS_ID)),
    ],
    ids=["conversion", "imaging", "preprocess"],
)
def test_a_preprocessing_body_is_sent_to_the_preprocessing_endpoint(
    client: gs.MwaAsvoClient,
    httpserver: HTTPServer,
    params: gs.ConversionJobParams | gs.ImagingJobParams | gs.PreprocessJobParams,
) -> None:
    serve_submissions(httpserver, PREPROCESS_PATH)

    (result,) = client.submit_many([ACCEPTED_OBS_ID], params)

    assert result.error is None
    assert obs_ids_sent_to(httpserver, PREPROCESS_PATH) == [ACCEPTED_OBS_ID]


def test_a_body_outside_the_limits_is_not_sent(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    """The result of each Obs ID is the library's check error, and nothing is sent."""
    requests_before = len(httpserver.log)

    results = client.submit_many(
        [ACCEPTED_OBS_ID, REFUSED_OBS_ID], gs.ConversionJobParams(ACCEPTED_OBS_ID, avg_freq_res=TOO_LARGE_FREQ_RES)
    )

    assert len(httpserver.log) == requests_before
    assert all(isinstance(result.error, gs.MwaAsvoApiErrorInvalidParameter) for result in results)
    assert all(result.response is None for result in results)


def test_no_obs_ids_submit_nothing(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    requests_before = len(httpserver.log)

    assert client.submit_many([], gs.DownloadJobParams(ACCEPTED_OBS_ID)) == []
    assert len(httpserver.log) == requests_before


def test_other_params_raise_type_error(client: gs.MwaAsvoClient) -> None:
    """An imaging body from a conversion job has no Obs ID, so it cannot be used."""
    with pytest.raises(TypeError, match="expected a DownloadJobParams"):
        client.submit_many([ACCEPTED_OBS_ID], gs.ImageFromJobParams(1, output_mode=TEST_OUTPUT_MODE))  # ty: ignore[invalid-argument-type]


def test_an_invalid_obs_id_raises_before_any_request(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    requests_before = len(httpserver.log)

    with pytest.raises(gs.ObsIdError):
        client.submit_many([ACCEPTED_OBS_ID, 1], gs.DownloadJobParams(ACCEPTED_OBS_ID))

    assert len(httpserver.log) == requests_before


def test_the_repr_shows_the_obs_id_and_the_outcome(client: gs.MwaAsvoClient, httpserver: HTTPServer) -> None:
    serve_submissions(httpserver, DOWNLOAD_PATH)

    refused, accepted = client.submit_many([REFUSED_OBS_ID, ACCEPTED_OBS_ID], gs.DownloadJobParams(ACCEPTED_OBS_ID))

    assert repr(refused).startswith(f"SubmitResult(obs_id={REFUSED_OBS_ID}, error=")
    assert repr(accepted).startswith(f"SubmitResult(obs_id={ACCEPTED_OBS_ID}, response=")
