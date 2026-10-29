"""Tests for the CSV classes and functions that match the Rust library: `CsvDefaults`, `CsvJob`, `CsvJobParams`,
`parse_manta_ray_csv`, `read_manta_ray_csv`, `submit_csv_jobs`, `submit_csv_job` and the CSV errors."""

import pathlib
from collections.abc import Callable
from typing import Any, cast

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .test_csv import (
    FIRST_OBS_ID,
    SECOND_OBS_ID,
    job_id_for,
    requests_to_submit_paths,
    serve_submissions,
)

# One line of each job type that the file format has.
LINE_BY_TYPE = {
    "Conversion": f"obs_id={FIRST_OBS_ID}, job_type=c",
    "Imaging": f"obs_id={FIRST_OBS_ID}, job_type=i",
    "Download": f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis",
    "Voltage": f"obs_id={FIRST_OBS_ID}, job_type=v, offset=0, duration=8",
    "Beamformer": f"obs_id={FIRST_OBS_ID}, job_type=b",
}
VISIBILITY_LINES = (
    f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\nobs_id={SECOND_OBS_ID}, job_type=d, download_type=vis\n"
)


def body(params: gs.CsvJobParams) -> Any:
    """The request body that a variant of `CsvJobParams` holds.

    Args:
        params: A variant of `CsvJobParams`.

    Returns:
        Its `params`.
    """
    return cast(Any, params).params


def test_defaults_have_no_values_unless_they_are_given() -> None:
    """Every default is `None`, which leaves the default of the MWA ASVO."""
    defaults = gs.CsvDefaults()

    assert defaults.delivery is None
    assert defaults.delivery_format is None
    assert defaults.allow_resubmit is None


def test_defaults_have_the_values_they_were_given_and_can_be_changed() -> None:
    """The attributes are the fields of the Rust struct. The arguments are keyword-only."""
    defaults = gs.CsvDefaults(delivery=gs.DeliveryDestination.Scratch, allow_resubmit=True)

    assert defaults.delivery == gs.DeliveryDestination.Scratch
    assert defaults.allow_resubmit is True
    defaults.delivery_format = gs.DeliveryFormat.Files
    assert defaults.delivery_format == gs.DeliveryFormat.Files
    with pytest.raises(TypeError):
        gs.CsvDefaults(gs.DeliveryDestination.Scratch)  # ty: ignore[too-many-positional-arguments]


def test_defaults_from_the_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """The delivery and its format come from the variables of the command. `allow_resubmit` does not."""
    monkeypatch.setenv(gs.ENV_GIANT_SQUID_DELIVERY, "scratch")
    monkeypatch.setenv(gs.ENV_GIANT_SQUID_DELIVERY_FORMAT, "files")

    defaults = gs.CsvDefaults.from_env()

    assert defaults.delivery == gs.DeliveryDestination.Scratch
    assert defaults.delivery_format == gs.DeliveryFormat.Files
    assert defaults.allow_resubmit is None


def test_defaults_from_an_empty_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """Every variable is optional."""
    monkeypatch.delenv(gs.ENV_GIANT_SQUID_DELIVERY, raising=False)
    monkeypatch.delenv(gs.ENV_GIANT_SQUID_DELIVERY_FORMAT, raising=False)

    assert gs.CsvDefaults.from_env().delivery is None


def test_defaults_from_an_environment_that_is_not_valid(monkeypatch: pytest.MonkeyPatch) -> None:
    """The error is of kind `InvalidEnvironment`, and names the variable."""
    monkeypatch.setenv(gs.ENV_GIANT_SQUID_DELIVERY, "the-moon")

    with pytest.raises(gs.AsvoError, match=gs.ENV_GIANT_SQUID_DELIVERY) as excinfo:
        gs.CsvDefaults.from_env()
    assert excinfo.value.kind == "InvalidEnvironment"


def test_parse_gives_a_job_for_each_line() -> None:
    """The jobs are in the order of the lines, and have their line numbers and Obs IDs."""
    jobs = gs.parse_manta_ray_csv(f"# a comment\n\n{VISIBILITY_LINES}")

    assert [(j.line, j.obs_id) for j in jobs] == [(3, FIRST_OBS_ID), (4, SECOND_OBS_ID)]
    assert all(isinstance(j, gs.CsvJob) for j in jobs)
    assert all(isinstance(j.obs_id, gs.ObsId) for j in jobs)


@pytest.mark.parametrize("variant", LINE_BY_TYPE)
def test_each_job_type_is_its_variant(variant: str) -> None:
    """`job_type` selects the variant of `CsvJobParams`, which holds the request body."""
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE[variant])

    assert isinstance(job.params, getattr(gs.CsvJobParams, variant))
    assert isinstance(job.params, gs.CsvJobParams)
    assert body(job.params).obs_id == FIRST_OBS_ID


def test_the_bodies_are_the_classes_of_the_submit_methods() -> None:
    """Each variant holds the class that the matching submit method takes."""
    jobs = {name: gs.parse_manta_ray_csv(line)[0].params for name, line in LINE_BY_TYPE.items()}

    assert isinstance(body(jobs["Conversion"]), gs.ConversionJobParams)
    assert isinstance(body(jobs["Imaging"]), gs.ImagingJobFlow1Params)
    for name in ("Download", "Voltage", "Beamformer"):
        assert isinstance(body(jobs[name]), gs.DownloadJobParams)
    assert body(jobs["Voltage"]).duration == 8


def test_the_keys_of_a_line_are_in_the_body() -> None:
    """A key that the line gives is in the body, and the defaults of the file fill the keys that it does not."""
    line = f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis_meta, delivery=dug"
    defaults = gs.CsvDefaults(delivery=gs.DeliveryDestination.Scratch, allow_resubmit=True)

    (job,) = gs.parse_manta_ray_csv(f"{line}\n", defaults)
    second = gs.parse_manta_ray_csv(f"obs_id={SECOND_OBS_ID}, job_type=d, download_type=vis\n", defaults)[0]

    assert body(job.params).only_get_metadata is True
    assert body(job.params).delivery == gs.DeliveryDestination.Dug
    assert body(second.params).delivery == gs.DeliveryDestination.Scratch
    assert body(second.params).allow_resubmit is True


@pytest.mark.parametrize(
    ("variant", "description"),
    [
        ("Conversion", "conversion"),
        ("Imaging", "imaging"),
        ("Download", "visibility download"),
        ("Voltage", "voltage"),
        ("Beamformer", "beamformer"),
    ],
)
def test_the_description_names_the_kind_of_job(variant: str, description: str) -> None:
    """`description()` is the library's words for the kind of job."""
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE[variant])

    assert job.params.description() == description


def test_the_description_of_a_metadata_download() -> None:
    """A download job with `only_get_metadata` is a metadata download."""
    (job,) = gs.parse_manta_ray_csv(f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis_meta")

    assert job.params.description() == "metadata download"


def test_validate_accepts_a_body_from_a_file_and_refuses_one_that_was_changed() -> None:
    """The check is the library's, on the body as it is now."""
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE["Voltage"])
    job.params.validate()

    body(job.params).offset = 6000

    with pytest.raises(ValueError, match="offset"):
        job.params.validate()


def test_a_job_can_be_made_by_hand() -> None:
    """A `CsvJob` has a line, an Obs ID and a body."""
    params = gs.CsvJobParams.Conversion(gs.ConversionJobParams(FIRST_OBS_ID))

    job = gs.CsvJob(7, FIRST_OBS_ID, params)

    assert (job.line, job.obs_id) == (7, FIRST_OBS_ID)
    assert job.params is params
    with pytest.raises(gs.ObsIdError):
        gs.CsvJob(7, 1, params)


def test_a_variant_can_be_matched() -> None:
    """The variants are classes, so `match` works as it does for a Rust enum."""
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE["Beamformer"])

    match job.params:
        case gs.CsvJobParams.Beamformer(params=body):
            assert isinstance(body, gs.DownloadJobParams)
        case _:
            pytest.fail("the variant was not matched")


def test_parse_of_a_file_without_a_job_is_an_empty_list() -> None:
    """Only `read_manta_ray_csv` refuses a file with no job."""
    assert gs.parse_manta_ray_csv("# nothing\n\n") == []


def test_parse_reports_every_bad_line() -> None:
    """`CsvError` has a `CsvRowError` for each line that has a problem."""
    text = (
        f"obs_id={FIRST_OBS_ID}, job_type=d, download_type=vis\n"
        f"obs_id={SECOND_OBS_ID}, job_type=c, no_rf=true\n"
        "obs_id=1, job_type=x\n"
    )

    with pytest.raises(gs.CsvError, match="2 problems in the CSV file") as excinfo:
        gs.parse_manta_ray_csv(text)

    errors = excinfo.value.errors
    assert all(isinstance(e, gs.CsvRowError) for e in errors)
    assert [e.line for e in errors] == [2, 3]
    assert "unknown key `no_rf`" in errors[0].message
    assert str(errors[0]) == f"line 2: {errors[0].message}"
    assert repr(errors[0]).startswith("CsvRowError(line=2, ")
    assert not isinstance(excinfo.value, ValueError)


def test_read_gives_the_jobs_of_a_file(write_csv: Callable[[str], pathlib.Path]) -> None:
    """`read_manta_ray_csv` reads the file and parses it."""
    jobs = gs.read_manta_ray_csv(write_csv(VISIBILITY_LINES))

    assert [j.obs_id for j in jobs] == [FIRST_OBS_ID, SECOND_OBS_ID]


def test_read_raises_os_error_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """A file that cannot be read is Python's own error, with the file name."""
    missing = tmp_path / "missing.csv"

    with pytest.raises(FileNotFoundError) as excinfo:
        gs.read_manta_ray_csv(missing)
    assert excinfo.value.filename == str(missing)


def test_read_refuses_a_file_with_no_job(write_csv: Callable[[str], pathlib.Path]) -> None:
    """The kind is `NoJobs`, and the error has the file."""
    path = write_csv("# nothing\n")

    with pytest.raises(gs.CsvFileErrorNoJobs, match="has no jobs") as excinfo:
        gs.read_manta_ray_csv(path)

    assert isinstance(excinfo.value, gs.CsvFileError)
    assert excinfo.value.kind == "NoJobs"
    assert excinfo.value.path == path


def test_read_refuses_a_file_with_a_bad_line(write_csv: Callable[[str], pathlib.Path]) -> None:
    """The kind is `Invalid`, and the error has the lines."""
    with pytest.raises(gs.CsvFileErrorInvalid) as excinfo:
        gs.read_manta_ray_csv(write_csv("obs_id=1, job_type=x\n"))

    assert isinstance(excinfo.value, gs.CsvFileError)
    assert excinfo.value.kind == "Invalid"
    assert [e.line for e in excinfo.value.errors] == [1]


def test_the_csv_errors_are_exceptions_that_are_not_asvo_errors() -> None:
    """The hierarchy is the library's."""
    for error in (gs.CsvError, gs.CsvJobsFailed, gs.CsvFileError, gs.CsvSubmitError):
        assert issubclass(error, Exception)
        assert not issubclass(error, (gs.AsvoApiError, gs.AsvoError, ValueError))
    assert issubclass(gs.CsvFileErrorInvalid, gs.CsvFileError)
    assert issubclass(gs.CsvFileErrorNoJobs, gs.CsvFileError)
    assert issubclass(gs.CsvSubmitErrorInvalid, gs.CsvSubmitError)
    assert issubclass(gs.CsvSubmitErrorNoJobs, gs.CsvSubmitError)
    assert issubclass(gs.CsvSubmitErrorFailed, gs.CsvSubmitError)


def test_submit_csv_file_raises_the_failed_kind(
    client: gs.AsvoClient,
    httpserver: HTTPServer,
    write_csv: Callable[[str], pathlib.Path],
) -> None:
    """A job that the server refuses is the kind `Failed`, with the replies and the failures."""
    serve_submissions(httpserver, failing=frozenset({FIRST_OBS_ID}))

    with pytest.raises(gs.CsvSubmitErrorFailed) as excinfo:
        client.submit_csv_file(write_csv(VISIBILITY_LINES))

    assert excinfo.value.kind == "Failed"
    assert [r.job_id for r in excinfo.value.submitted] == [job_id_for(SECOND_OBS_ID)]
    assert [f.obs_id for f in excinfo.value.failures] == [FIRST_OBS_ID]


def test_submit_csv_jobs_submits_the_jobs_in_order(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """The replies are in the order of the jobs, and each job goes to the endpoint of its type."""
    serve_submissions(httpserver)
    jobs = gs.parse_manta_ray_csv(f"{LINE_BY_TYPE['Conversion']}\n{VISIBILITY_LINES}")

    replies = client.submit_csv_jobs(jobs)

    assert [r.job_id for r in replies] == [
        job_id_for(FIRST_OBS_ID),
        job_id_for(FIRST_OBS_ID),
        job_id_for(SECOND_OBS_ID),
    ]
    paths = [path for path, _ in requests_to_submit_paths(httpserver)]
    assert paths[0] != paths[1]
    assert paths[1] == paths[2]


def test_submit_csv_jobs_calls_on_result_for_each_job(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """The function gets the job and its reply, or the `AsvoApiError` of a failed request as a value."""
    serve_submissions(httpserver, failing=frozenset({FIRST_OBS_ID}))
    jobs = gs.parse_manta_ray_csv(VISIBILITY_LINES)
    seen: list[tuple[gs.CsvJob, object]] = []

    with pytest.raises(gs.CsvJobsFailed) as excinfo:
        client.submit_csv_jobs(jobs, lambda job, result: seen.append((job, result)))

    assert [job for job, _ in seen] == jobs
    assert isinstance(seen[0][1], gs.AsvoApiError)
    assert isinstance(seen[1][1], gs.JobSubmittedResponse)
    assert seen[1][1].job_id == job_id_for(SECOND_OBS_ID)
    assert [r.job_id for r in excinfo.value.submitted] == [job_id_for(SECOND_OBS_ID)]
    assert [f.obs_id for f in excinfo.value.failures] == [FIRST_OBS_ID]
    assert not isinstance(excinfo.value, gs.CsvSubmitError)


def test_an_exception_in_on_result_is_raised_when_the_jobs_are_done(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """The library's loop cannot be stopped, so the other jobs are submitted, and the function is not called again."""
    serve_submissions(httpserver)
    jobs = gs.parse_manta_ray_csv(VISIBILITY_LINES)
    calls: list[gs.CsvJob] = []

    def on_result(job: gs.CsvJob, result: object) -> None:
        calls.append(job)
        raise RuntimeError("the callback failed")

    with pytest.raises(RuntimeError, match="the callback failed"):
        client.submit_csv_jobs(jobs, on_result)

    assert len(calls) == 1
    assert len(requests_to_submit_paths(httpserver)) == len(jobs)


def test_submit_csv_jobs_refuses_a_body_that_is_not_valid_and_sends_nothing(
    client: gs.AsvoClient, httpserver: HTTPServer
) -> None:
    """A body that was changed after it was read is checked before any request."""
    serve_submissions(httpserver)
    jobs = gs.parse_manta_ray_csv(f"{LINE_BY_TYPE['Imaging']}\n")
    body(jobs[0].params).image_size = 100

    with pytest.raises(ValueError, match="image_size"):
        client.submit_csv_jobs(jobs)

    assert requests_to_submit_paths(httpserver) == []


def test_submit_csv_job_submits_one_body(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """One job goes to its endpoint, and its reply comes back."""
    serve_submissions(httpserver)
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE["Voltage"])

    reply = client.submit_csv_job(job.params)

    assert isinstance(reply, gs.JobSubmittedResponse)
    assert reply.job_id == job_id_for(FIRST_OBS_ID)
    ((_, body),) = requests_to_submit_paths(httpserver)
    assert body["obs_id"] == FIRST_OBS_ID
    assert body["duration"] == 8


def test_submit_csv_job_raises_the_api_error_of_a_failed_request(client: gs.AsvoClient, httpserver: HTTPServer) -> None:
    """A failed request is an `AsvoApiError`, as for the other submit methods."""
    serve_submissions(httpserver, failing=frozenset({FIRST_OBS_ID}))
    (job,) = gs.parse_manta_ray_csv(LINE_BY_TYPE["Download"])

    with pytest.raises(gs.AsvoApiError) as excinfo:
        client.submit_csv_job(job.params)
    assert excinfo.value.kind == "ApiError"
