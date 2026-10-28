"""Tests for `DownloadOptions`."""

import pathlib

import pytest

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


def test_the_other_arguments_are_keyword_only(tmp_path: pathlib.Path) -> None:
    """The flags are named."""
    with pytest.raises(TypeError):
        gs.DownloadOptions(tmp_path, True)  # ty: ignore[too-many-positional-arguments]


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


def test_the_options_can_be_changed(tmp_path: pathlib.Path) -> None:
    """The attributes are the fields of the Rust struct, and they can be set. One set of options serves a series."""
    options = gs.DownloadOptions(tmp_path)

    options.keep_tar = True
    options.download_number = 5
    options.progress = print
    options.download_dir = tmp_path / "other"

    assert options.keep_tar is True
    assert options.download_number == 5
    assert options.progress is print
    assert options.download_dir == tmp_path / "other"
    options.progress = None
    assert options.progress is None


def test_the_options_can_use_the_download_settings(tmp_path: pathlib.Path) -> None:
    """The settings of the environment are the buffer size and the retry duration of the options."""
    settings = gs.DownloadSettings(buffer_size=BUFFER_SIZE, retry_duration=RETRY_SECONDS)

    options = gs.DownloadOptions(tmp_path, buffer_size=settings.buffer_size, retry_duration=settings.retry_duration)

    assert (options.buffer_size, options.retry_duration) == (BUFFER_SIZE, RETRY_SECONDS)


def test_the_repr_shows_the_values(tmp_path: pathlib.Path) -> None:
    """The repr names the fields."""
    text = repr(gs.DownloadOptions(tmp_path, keep_tar=True))

    assert text.startswith("DownloadOptions(")
    assert "keep_tar=true" in text
