"""Tests for the mwa_giant_squid module itself: import, version and logging."""

import importlib.metadata
from typing import TypeVar

import pytest

import mwa_giant_squid

# A member of one of the enums of the module.
_Member = TypeVar("_Member")

# The PyPI distribution name (the import name is mwa_giant_squid).
DISTRIBUTION_NAME = "mwa-giant-squid"


def test_version_matches_the_installed_distribution() -> None:
    """The module's __version__ is the crate version, which maturin also uses for the wheel."""
    assert mwa_giant_squid.__version__ == importlib.metadata.version(DISTRIBUTION_NAME)


def test_the_constants_are_the_librarys() -> None:
    """The programs on top of the module (the `giant-squid` command) take these from the library, not a copy."""
    assert mwa_giant_squid.ENV_GIANT_SQUID_DELIVERY == "GIANT_SQUID_DELIVERY"
    assert mwa_giant_squid.ENV_GIANT_SQUID_DELIVERY_FORMAT == "GIANT_SQUID_DELIVERY_FORMAT"
    assert mwa_giant_squid.ENDPOINT_JOBS == "/api/v2/jobs"
    assert mwa_giant_squid.ENDPOINT_DOWNLOAD_VIS_JOB == "/api/v2/download_vis_job"
    assert mwa_giant_squid.ENDPOINT_CONVERSION_JOB == "/api/v2/conversion_job"
    assert mwa_giant_squid.ENDPOINT_IMAGING_JOB == "/api/v2/imaging_job"
    assert mwa_giant_squid.ENDPOINT_IMAGE_FROM_JOB == "/api/v2/image_from_job"
    assert mwa_giant_squid.ENDPOINT_VOLTAGE_JOB == "/api/v2/voltage_job"
    assert mwa_giant_squid.ENDPOINT_BEAMFORMER_JOB == "/api/v2/beamformer_job"
    assert mwa_giant_squid.WAIT_POLL_INTERVAL_SECS == 60.0
    assert mwa_giant_squid.WAIT_INITIAL_DELAY_SECS == 1.0
    assert mwa_giant_squid.DEFAULT_CONCURRENT_DOWNLOADS == 4


def test_reset_logging_can_be_called() -> None:
    """reset_logging exists and can be called more than once."""
    mwa_giant_squid.reset_logging()
    mwa_giant_squid.reset_logging()


def test_job_state_names_are_listed_and_parsed() -> None:
    """``AsvoJobState.names()`` lists the names that ``parse`` accepts, and every state has one."""
    names = mwa_giant_squid.AsvoJobState.names()

    assert names[:3] == ["queued", "waitcal", "staging"]
    assert len(names) == len(set(names))
    parsed = [mwa_giant_squid.AsvoJobState.parse(name) for name in names]
    assert len(set(parsed)) == len(list(_members(mwa_giant_squid.AsvoJobState)))
    assert mwa_giant_squid.AsvoJobState.parse("WAIT-CAL") is mwa_giant_squid.AsvoJobState.WaitCal
    assert mwa_giant_squid.AsvoJobState.parse("error") is mwa_giant_squid.AsvoJobState.Error


def test_job_type_names_are_listed_and_parsed() -> None:
    """``AsvoJobType.names()`` lists the names that ``parse`` accepts; ``Unknown`` has none."""
    names = mwa_giant_squid.AsvoJobType.names()

    assert "download_voltages" in names
    assert "download_voltage" not in names
    assert len(names) == len(set(names))
    parsed = {mwa_giant_squid.AsvoJobType.parse(name) for name in names}
    assert parsed == set(_members(mwa_giant_squid.AsvoJobType)) - {mwa_giant_squid.AsvoJobType.Unknown}
    assert (
        mwa_giant_squid.AsvoJobType.parse("Download_Visibilities") is mwa_giant_squid.AsvoJobType.DownloadVisibilities
    )
    assert mwa_giant_squid.AsvoJobType.parse("download_voltage") is mwa_giant_squid.AsvoJobType.DownloadVoltage


def test_a_text_that_is_not_a_name_raises_the_library_error() -> None:
    """``parse`` raises ``AsvoError`` with the kind and the text that could not be parsed."""
    with pytest.raises(mwa_giant_squid.AsvoError) as state_error:
        mwa_giant_squid.AsvoJobState.parse("bogus")
    assert state_error.value.kind == "InvalidJobState"
    assert state_error.value.str == "bogus"

    for text in ("bogus", "unknown"):
        with pytest.raises(mwa_giant_squid.AsvoError) as type_error:
            mwa_giant_squid.AsvoJobType.parse(text)
        assert type_error.value.kind == "InvalidJobType"
        assert type_error.value.str == text


def _members(enum_type: type[_Member]) -> list[_Member]:
    """The members of a module enum."""
    return [member for name in dir(enum_type) if isinstance(member := getattr(enum_type, name), enum_type)]
