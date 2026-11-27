"""Tests for `DownloadOptions`."""

import pathlib

import mwa_giant_squid as gs

BUFFER_SIZE = 4096
RETRY_SECONDS = 2.5


def test_the_options_have_the_defaults_of_the_old_arguments(tmp_path: pathlib.Path) -> None:
    """Only the directory is needed. The files go in a directory of their own, and are hashed."""
    options = gs.DownloadOptions(tmp_path)

    assert options.download_dir == tmp_path
    assert isinstance(options.download_dir, pathlib.Path)
    assert options.keep_tar is False
    assert options.job_subdirectory is True
    assert options.no_resume is False
    assert options.hash is True
    assert options.progress is None
    assert options.buffer_size == gs.DEFAULT_DOWNLOAD_BUFFER_SIZE
    assert options.retry_duration == gs.DEFAULT_DOWNLOAD_RETRY_DURATION
    assert options.download_number == 1
    assert options.download_count == 1


def test_the_directory_can_be_a_string(tmp_path: pathlib.Path) -> None:
    """A `str` and a `Path` are both accepted, and it comes back as a `Path`."""
    assert gs.DownloadOptions(str(tmp_path)).download_dir == tmp_path


def test_the_options_have_the_values_they_were_given(tmp_path: pathlib.Path) -> None:
    """Each value comes back as it went in."""
    events: list[gs.DownloadProgress] = []
    options = gs.DownloadOptions(
        tmp_path,
        keep_tar=True,
        job_subdirectory=False,
        no_resume=True,
        hash=False,
        progress=events.append,
        buffer_size=BUFFER_SIZE,
        retry_duration=RETRY_SECONDS,
        download_number=2,
        download_count=3,
    )

    assert options.keep_tar is True
    assert options.job_subdirectory is False
    assert options.no_resume is True
    assert options.hash is False
    assert options.progress == events.append
    assert options.buffer_size == BUFFER_SIZE
    assert options.retry_duration == RETRY_SECONDS
    assert (options.download_number, options.download_count) == (2, 3)
    # The progress callback can be changed, and removed.
    options.progress = print
    assert options.progress is print
    options.progress = None
    assert options.progress is None


def test_the_repr_shows_the_values(tmp_path: pathlib.Path) -> None:
    """The repr names the fields."""
    text = repr(gs.DownloadOptions(tmp_path, keep_tar=True))

    assert text.startswith("DownloadOptions(")
    assert "keep_tar=true" in text
