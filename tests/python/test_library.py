"""Tests of the constants and the plain functions that the module takes from the Rust library."""

import hashlib
import pathlib

import pytest

import mwa_giant_squid as gs

# The seconds in a Rust `Duration` that the module gives as a float.
DEFAULT_API_TIMEOUT_SECONDS = 60.0
DEFAULT_DOWNLOAD_RETRY_SECONDS = 900.0

BYTES_PER_MIB = 1024 * 1024

# The names of the environment variables that the library reads.
ENV_NAMES = {
    "ENV_MWA_ASVO_HOST": "MWA_ASVO_HOST",
    "ENV_MWA_ASVO_API_KEY": "MWA_ASVO_API_KEY",
    "ENV_MWA_ASVO_API_TIMEOUT": "MWA_ASVO_API_TIMEOUT",
    "ENV_HOME": "HOME",
    "ENV_GIANT_SQUID_BUF_SIZE": "GIANT_SQUID_BUF_SIZE",
    "ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS": "GIANT_SQUID_DOWNLOAD_RETRY_SECS",
    "ENV_GIANT_SQUID_DELIVERY": "GIANT_SQUID_DELIVERY",
    "ENV_GIANT_SQUID_DELIVERY_FORMAT": "GIANT_SQUID_DELIVERY_FORMAT",
}

JOB_ID = 1234
FILE_CONTENT = b"abc"
FILE_SHA1 = hashlib.sha1(FILE_CONTENT).hexdigest()


# The constants of the library and their values: a Rust `Duration` is a float of seconds, and the sizes are bytes.
CONSTANTS = {
    "DEFAULT_API_TIMEOUT": DEFAULT_API_TIMEOUT_SECONDS,
    "DEFAULT_DOWNLOAD_RETRY_DURATION": DEFAULT_DOWNLOAD_RETRY_SECONDS,
    "BYTES_PER_MIB": BYTES_PER_MIB,
    "MIN_DOWNLOAD_BUFFER_SIZE": 1,
    "DEFAULT_DOWNLOAD_BUFFER_SIZE": 100 * BYTES_PER_MIB,
    "DEFAULT_MWA_ASVO_HOST": "https://asvo.mwatelescope.org:443",
    "SIGNED_URL_LIFETIME_DAYS": 7,
}


@pytest.mark.parametrize(("constant", "value"), CONSTANTS.items())
def test_the_constants_have_the_values_of_the_library(constant: str, value: object) -> None:
    """Each constant has the value of the Rust library."""
    assert getattr(gs, constant) == value


def test_the_obs_id_hint_names_the_list_command() -> None:
    """The hint that the Rust parsers add to a message is the text of the library."""
    assert "giant-squid list" in gs.OBS_ID_HINT


@pytest.mark.parametrize(("constant", "name"), ENV_NAMES.items())
def test_the_environment_variable_names(constant: str, name: str) -> None:
    """Each ENV_ constant is the name of a variable."""
    assert getattr(gs, constant) == name


def test_default_token_cache_path_is_in_the_home_directory(tmp_path: pathlib.Path) -> None:
    """The cache is `<home>/.mwa-asvo/tokens.json`, the file that mwa-cli uses, and the result is a Path."""
    path = gs.default_token_cache_path(tmp_path)
    assert isinstance(path, pathlib.Path)
    assert path == tmp_path / ".mwa-asvo" / "tokens.json"


def test_default_token_cache_path_takes_a_string(tmp_path: pathlib.Path) -> None:
    """A `str` home works as a Path does."""
    assert gs.default_token_cache_path(str(tmp_path)) == tmp_path / ".mwa-asvo" / "tokens.json"


def test_default_token_cache_path_makes_no_file(tmp_path: pathlib.Path) -> None:
    """The function only builds the path."""
    gs.default_token_cache_path(tmp_path)
    assert list(tmp_path.iterdir()) == []


@pytest.fixture
def hashed_file(tmp_path: pathlib.Path) -> pathlib.Path:
    """A file that has a known content."""
    path = tmp_path / "file.dat"
    path.write_bytes(FILE_CONTENT)
    return path


def test_check_file_sha1_hash_accepts_the_right_hash(hashed_file: pathlib.Path) -> None:
    """A matching hash gives `None`."""
    assert gs.check_file_sha1_hash(hashed_file, FILE_SHA1, JOB_ID) is None


def test_check_file_sha1_hash_ignores_the_case_of_the_hash(hashed_file: pathlib.Path) -> None:
    """The MWA ASVO's hash can be in either case."""
    assert gs.check_file_sha1_hash(hashed_file, FILE_SHA1.upper(), JOB_ID) is None


def test_check_file_sha1_hash_takes_a_string_path(hashed_file: pathlib.Path) -> None:
    """The file can be a `str`."""
    assert gs.check_file_sha1_hash(str(hashed_file), FILE_SHA1, JOB_ID) is None


def test_check_file_sha1_hash_raises_for_a_wrong_hash(hashed_file: pathlib.Path) -> None:
    """A wrong hash is an `MwaAsvoError` of kind `HashMismatch`, with both hashes."""
    wrong = "0" * len(FILE_SHA1)
    with pytest.raises(gs.MwaAsvoError) as excinfo:
        gs.check_file_sha1_hash(hashed_file, wrong, JOB_ID)
    error = excinfo.value
    assert error.kind == "HashMismatch"
    assert error.job_id == JOB_ID
    assert error.file == str(hashed_file)
    assert error.calculated_hash == FILE_SHA1
    assert error.expected_hash == wrong


def test_check_file_sha1_hash_raises_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """A file that is not there is an `OSError`."""
    with pytest.raises(FileNotFoundError):
        gs.check_file_sha1_hash(tmp_path / "missing.dat", FILE_SHA1, JOB_ID)


def test_check_file_sha1_hash_refuses_job_id_zero(hashed_file: pathlib.Path) -> None:
    """0 is not a Job ID."""
    with pytest.raises(gs.ParseErrorZeroJobId, match="0 is not a Job ID or an Obs ID"):
        gs.check_file_sha1_hash(hashed_file, FILE_SHA1, 0)
