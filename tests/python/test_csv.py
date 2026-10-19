"""Tests for ``AsvoClient.submit_csv_file``, against a local mock MWA ASVO.

The parser and the submit loop are the Rust library's, and its tests cover them in detail. These tests check what
the Python module adds: the argument types, the replies as Python objects, and the Python exceptions.
"""

import errno
import json
import logging
import pathlib
from collections.abc import Callable

import pytest
from pytest_httpserver import HTTPServer
from werkzeug import Request, Response

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, error_response

# The manta-ray-client files that the project keeps for its tests, and the endpoint of the jobs of each.
SAMPLE_DIR = pathlib.Path(__file__).resolve().parents[1] / "files" / "manta-ray-client"
SAMPLE_FILES = [
    ("beamformer_jobs.csv", "/api/v2/beamformer_job"),
    ("conversion_jobs.csv", "/api/v2/conversion_job"),
    ("imaging_jobs.csv", "/api/v2/imaging_job"),
    ("meta_jobs.csv", "/api/v2/download_vis_job"),
    ("visibility_jobs.csv", "/api/v2/download_vis_job"),
    ("voltage_jobs.csv", "/api/v2/voltage_job"),
]
SUBMIT_PATHS = sorted({path for _, path in SAMPLE_FILES})

# Two Obs IDs used in the files of the tests.
FIRST_OBS_ID = 1349904240
SECOND_OBS_ID = 1349904184

# The Job ID that the mock gives each Obs ID, so that a test can tell the replies apart.
JOB_ID_MODULUS = 1000


def job_id_for(obs_id: int) -> int:
    """The Job ID that the mock gives to the job of ``obs_id``.

    Args:
        obs_id: The Obs ID of the job.

    Returns:
        A Job ID that is more than 0.
    """
    return obs_id % JOB_ID_MODULUS + 1


def serve_submissions(httpserver: HTTPServer, failing: frozenset[int] = frozenset()) -> None:
    """Serve every submit endpoint: a reply for each job, or an error for the Obs IDs in ``failing``.

    Args:
        httpserver: The pytest-httpserver server.
        failing: The Obs IDs whose job the mock refuses.
    """

    def handler(request: Request) -> Response:
        obs_id = request.get_json()["obs_id"]
        if obs_id in failing:
            return Response(
                json.dumps(error_response("VALID_MISSING_FIELD", "Bad obsid")),
                status=400,
                content_type="application/json",
            )
        body = {"job_id": job_id_for(obs_id), "message": "Job submitted", "status": "success"}
        return Response(json.dumps(body), content_type="application/json")

    for path in SUBMIT_PATHS:
        httpserver.expect_request(path, method="POST").respond_with_handler(handler)


def requests_to_submit_paths(httpserver: HTTPServer) -> list[tuple[str, dict[str, object]]]:
    """The submit requests that the mock received, as (path, JSON body), in order.

    Args:
        httpserver: The pytest-httpserver server.

    Returns:
        The requests.
    """
    return [
        (request.path, json.loads(request.get_data())) for request, _ in httpserver.log if request.path in SUBMIT_PATHS
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
    return gs.AsvoClient(host, TEST_API_KEY)


@pytest.fixture
def write_csv(tmp_path: pathlib.Path) -> Callable[[str], pathlib.Path]:
    """A function that writes a CSV file in a temporary directory.

    Args:
        tmp_path: The test's temporary directory.

    Returns:
        The function, which returns the path of the file.
    """

    def write(text: str) -> pathlib.Path:
        path = tmp_path / "jobs.csv"
        path.write_text(text)
        return path

    return write


@pytest.mark.parametrize(("file", "path"), SAMPLE_FILES)
def test_every_manta_ray_file_is_submitted(client: gs.AsvoClient, httpserver: HTTPServer, file: str, path: str) -> None:
    """Each file that manta-ray-client users have gives two replies, from the endpoint of its job type."""
    serve_submissions(httpserver)

    replies = client.submit_csv_file(SAMPLE_DIR / file)

    assert len(replies) == 2
    assert all(isinstance(reply, gs.JobSubmittedResponse) for reply in replies)
    assert [p for p, _ in requests_to_submit_paths(httpserver)] == [path, path]


def test_the_replies_are_in_the_order_of_the_file(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """A reply is a JobSubmittedResponse, and the list follows the lines."""
    serve_submissions(httpserver)
    path = write_csv(
        f"# a comment\nobs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\nobs_id={SECOND_OBS_ID}, job_type=c\n"
    )

    replies = client.submit_csv_file(path)

    assert [reply.job_id for reply in replies] == [job_id_for(FIRST_OBS_ID), job_id_for(SECOND_OBS_ID)]
    assert all(reply.status == gs.Status.Success for reply in replies)
    assert [p for p, _ in requests_to_submit_paths(httpserver)] == [
        "/api/v2/download_vis_job",
        "/api/v2/conversion_job",
    ]


def test_the_path_can_be_a_string(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """A path is a ``str`` or a ``PathLike``."""
    serve_submissions(httpserver)
    path = write_csv(f"obs_id={FIRST_OBS_ID}, job_type=b\n")

    assert len(client.submit_csv_file(str(path))) == 1


def test_a_file_with_no_job_gives_an_empty_list(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """Comments and blank lines are not jobs, and an empty list is not an error."""
    serve_submissions(httpserver)

    assert client.submit_csv_file(write_csv("# nothing\n\n")) == []
    assert requests_to_submit_paths(httpserver) == []


def test_the_arguments_are_the_defaults_of_the_lines_that_give_no_value(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """A line that gives a key wins over the argument."""
    serve_submissions(httpserver)
    path = write_csv(
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\n"
        f"obs_id={SECOND_OBS_ID}, job_type=d, download_type=vis, delivery=dug, delivery_format=tar, allow_resubmit=false\n"
    )

    client.submit_csv_file(
        path, delivery=gs.Delivery.Scratch, delivery_format=gs.DeliveryFormat.Files, allow_resubmit=True
    )

    first, second = (body for _, body in requests_to_submit_paths(httpserver))
    assert (first["delivery"], first["delivery_format"], first["allow_resubmit"]) == ("scratch", "files", True)
    assert (second["delivery"], second["delivery_format"], second["allow_resubmit"]) == ("dug", "tar", False)


def test_a_bad_line_raises_value_error_and_sends_nothing(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """The message names every bad line, and no job is submitted."""
    serve_submissions(httpserver)
    path = write_csv(
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\n"
        f"obs_id={SECOND_OBS_ID}, job_type=c, no_rf=true\n"
        f"obs_id={SECOND_OBS_ID}, job_type=x\n"
    )

    with pytest.raises(ValueError, match="2 problems in the CSV file") as info:
        client.submit_csv_file(path)

    assert "line 2: unknown key `no_rf`" in str(info.value)
    assert "line 3: job_type" in str(info.value)
    assert requests_to_submit_paths(httpserver) == []


def test_a_file_that_cannot_be_read_raises_the_os_error_of_open(client: gs.AsvoClient, tmp_path: pathlib.Path) -> None:
    """A missing file is a FileNotFoundError that has the file name."""
    missing = tmp_path / "no_such_file.csv"

    with pytest.raises(FileNotFoundError) as info:
        client.submit_csv_file(missing)

    assert info.value.filename == str(missing)
    assert str(missing) in str(info.value)
    assert info.value.errno == errno.ENOENT


def test_a_failed_job_does_not_stop_the_others_and_the_exception_has_the_replies(
    client: gs.AsvoClient, httpserver: HTTPServer, write_csv: Callable[[str], pathlib.Path]
) -> None:
    """CsvSubmitError has the replies of the jobs that were submitted, and a CsvJobFailure for the others."""
    serve_submissions(httpserver, failing=frozenset({FIRST_OBS_ID}))
    path = write_csv(
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\nobs_id={SECOND_OBS_ID}, job_type=d, download_type=vis\n"
    )

    with pytest.raises(gs.CsvSubmitError, match="1 of 2 job submissions failed") as info:
        client.submit_csv_file(path)

    assert len(requests_to_submit_paths(httpserver)) == 2
    (reply,) = info.value.submitted
    assert isinstance(reply, gs.JobSubmittedResponse)
    assert reply.job_id == job_id_for(SECOND_OBS_ID)
    (failure,) = info.value.failures
    assert isinstance(failure, gs.CsvJobFailure)
    assert (failure.line, failure.obs_id) == (1, FIRST_OBS_ID)
    assert isinstance(failure.error, gs.AsvoApiError)
    assert failure.error.kind == "ApiError"
    assert failure.error.error_code == "VALID_MISSING_FIELD"
    assert f"line=1, obs_id={FIRST_OBS_ID}" in repr(failure)


def test_the_exception_is_an_exception_that_is_not_an_asvo_error() -> None:
    """A caller that catches the module's other exceptions does not catch a partly failed file by mistake."""
    assert issubclass(gs.CsvSubmitError, Exception)
    assert not issubclass(gs.CsvSubmitError, (gs.AsvoApiError, gs.AsvoError))


def test_a_repeated_key_and_a_repeated_job_are_logged_as_warnings(
    client: gs.AsvoClient,
    httpserver: HTTPServer,
    write_csv: Callable[[str], pathlib.Path],
    caplog: pytest.LogCaptureFixture,
) -> None:
    """The warnings of the library go to Python's logging, and every job is still submitted."""
    serve_submissions(httpserver)
    path = write_csv(
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis, download_type=vis_meta\n"
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis_meta\n"
    )
    caplog.set_level(logging.WARNING)
    gs.reset_logging()
    try:
        replies = client.submit_csv_file(path)
    finally:
        gs.reset_logging()

    assert len(replies) == 2
    messages = [record.getMessage() for record in caplog.records]
    assert "line 1: the key `download_type` is given twice; the last value is used" in messages
    assert "line 2: this line makes the same job as line 1" in messages
