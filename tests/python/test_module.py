"""Tests for the mwa_giant_squid module itself: import, version and logging."""

import importlib.metadata

import mwa_giant_squid

# The PyPI distribution name (the import name is mwa_giant_squid).
DISTRIBUTION_NAME = "mwa-giant-squid"


def test_version_matches_the_installed_distribution() -> None:
    """The module's __version__ is the crate version, which maturin also uses for the wheel."""
    assert mwa_giant_squid.__version__ == importlib.metadata.version(DISTRIBUTION_NAME)


def test_reset_logging_can_be_called() -> None:
    """reset_logging exists and can be called more than once."""
    mwa_giant_squid.reset_logging()
    mwa_giant_squid.reset_logging()
