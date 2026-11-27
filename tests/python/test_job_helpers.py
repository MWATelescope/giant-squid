"""Tests for `MwaAsvoJob.total_size` and `MwaAsvoJob.download_destination`."""

import pathlib
from collections.abc import Callable
from typing import Any

import pytest

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY, job_detail

ACACIA_JOB_ID = 301
SCRATCH_JOB_ID = 302
QUEUED_JOB_ID = 303
EMPTY_JOB_ID = 304

FILE_SIZES = [1000, 2345]
FILE_SHA1 = "0123456789abcdef0123456789abcdef01234567"
FILE_URL = "https://example.org/file.tar?Expires=1790000000"
SCRATCH_PATH = f"/scratch/mwaops/asvo/{SCRATCH_JOB_ID}"


def served_jobs() -> list[dict[str, Any]]:
    """An Acacia job with two files, a Scratch job, a queued job with no product, and a product with no files."""
    acacia_files = [{"type": "acacia", "url": FILE_URL, "size": size, "sha1": FILE_SHA1} for size in FILE_SIZES]
    scratch_files = [{"type": "scratch", "path": SCRATCH_PATH, "size": FILE_SIZES[0]}]
    return [
        job_detail(ACACIA_JOB_ID, "completed", product={"files": acacia_files}),
        job_detail(SCRATCH_JOB_ID, "completed", product={"files": scratch_files}),
        job_detail(QUEUED_JOB_ID, "queued"),
        job_detail(EMPTY_JOB_ID, "completed", product={"files": []}),
    ]


@pytest.fixture
def jobs(host: str, mock_login: None, serve_jobs: Callable[..., None]) -> dict[int, gs.MwaAsvoJob]:
    """The jobs of `served_jobs`, got through a client, by Job ID."""
    serve_jobs(served_jobs())
    client = gs.MwaAsvoClient(gs.MwaAsvoClientConfig(host, TEST_API_KEY))
    return {job.job_id: job for job in client.get_jobs()}


def test_the_total_size_is_the_sum_of_the_file_sizes(jobs: dict[int, gs.MwaAsvoJob]) -> None:
    assert jobs[ACACIA_JOB_ID].total_size == sum(FILE_SIZES)


def test_a_job_with_no_product_has_no_total_size(jobs: dict[int, gs.MwaAsvoJob]) -> None:
    assert jobs[QUEUED_JOB_ID].total_size is None


def test_a_product_with_no_files_has_a_total_size_of_zero(jobs: dict[int, gs.MwaAsvoJob]) -> None:
    assert jobs[EMPTY_JOB_ID].total_size == 0


def test_an_acacia_job_goes_into_its_job_directory(jobs: dict[int, gs.MwaAsvoJob], tmp_path: pathlib.Path) -> None:
    """The default is the default of `DownloadOptions.job_subdirectory`."""
    job = jobs[ACACIA_JOB_ID]

    assert job.download_destination(tmp_path) == tmp_path / str(ACACIA_JOB_ID)
    assert job.download_destination(tmp_path, job_subdirectory=True) == tmp_path / str(ACACIA_JOB_ID)
    assert job.download_destination(tmp_path, job_subdirectory=False) == tmp_path
    assert gs.DownloadOptions(tmp_path).job_subdirectory is True


def test_a_scratch_job_goes_into_the_download_directory(jobs: dict[int, gs.MwaAsvoJob], tmp_path: pathlib.Path) -> None:
    job = jobs[SCRATCH_JOB_ID]

    assert job.download_destination(tmp_path) == tmp_path
    assert job.download_destination(tmp_path, job_subdirectory=False) == tmp_path


def test_the_destination_takes_a_string_and_makes_no_directory(
    jobs: dict[int, gs.MwaAsvoJob], tmp_path: pathlib.Path
) -> None:
    destination = jobs[ACACIA_JOB_ID].download_destination(str(tmp_path))

    assert isinstance(destination, pathlib.Path)
    assert not destination.exists()
