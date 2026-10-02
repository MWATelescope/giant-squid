"""Tests for the mwa_giant_squid module itself: import, version and logging."""

import importlib.metadata

import mwa_giant_squid

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
