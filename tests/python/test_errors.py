"""Tests for the exception classes of `AsvoApiError` and `AsvoError`: a subclass for each Rust variant."""

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import LOGIN_PATH, TEST_API_KEY, TEST_OBS_ID
from .parity_manifest import API_ERROR_VARIANTS, ERROR_VARIANTS, EXCLUDED_ERROR_VARIANTS

# The variants that have a class: the others raise OSError.
ERROR_VARIANTS_WITH_A_CLASS = [v for v in ERROR_VARIANTS if v not in EXCLUDED_ERROR_VARIANTS]


@pytest.mark.parametrize("variant", API_ERROR_VARIANTS)
def test_each_api_error_variant_has_a_subclass(variant: str) -> None:
    """The class has the names of the base class and the variant, and is an `AsvoApiError`."""
    cls = getattr(gs, f"AsvoApiError{variant}")

    assert issubclass(cls, gs.AsvoApiError)
    assert not issubclass(cls, ValueError)
    assert cls.__module__ == "mwa_giant_squid"


@pytest.mark.parametrize("variant", ERROR_VARIANTS_WITH_A_CLASS)
def test_each_error_variant_has_a_subclass(variant: str) -> None:
    """The class has the names of the base class and the variant, and is an `AsvoError`."""
    cls = getattr(gs, f"AsvoError{variant}")

    assert issubclass(cls, gs.AsvoError)
    assert not issubclass(cls, ValueError)
    assert cls.__module__ == "mwa_giant_squid"


@pytest.mark.parametrize("variant", EXCLUDED_ERROR_VARIANTS)
def test_a_variant_that_python_raises_as_os_error_has_no_class(variant: str) -> None:
    """The I/O variants are `OSError`, with its subclasses."""
    assert not hasattr(gs, f"AsvoError{variant}")


def test_the_base_classes_are_exceptions() -> None:
    """The two bases are plain exceptions, and not each other's subclass."""
    assert issubclass(gs.AsvoApiError, Exception)
    assert issubclass(gs.AsvoError, Exception)
    assert not issubclass(gs.AsvoError, gs.AsvoApiError)
    assert not issubclass(gs.AsvoApiError, gs.AsvoError)


def test_an_invalid_parameter_is_an_api_error_that_is_not_a_value_error() -> None:
    """A job argument outside the schema limits is an `AsvoApiError` of kind `InvalidParameter`."""
    with pytest.raises(gs.AsvoApiErrorInvalidParameter) as excinfo:
        gs.JobsFilter(job_ids=[1], obs_ids=[TEST_OBS_ID]).validate()

    error = excinfo.value
    assert isinstance(error, gs.AsvoApiError)
    assert not isinstance(error, ValueError)
    assert error.kind == "InvalidParameter"
    assert error.name == "job_ids"
    assert "Job IDs and Obs IDs" in error.message
    assert str(error) == f"Invalid {error.name}: {error.message}"


def test_an_invalid_job_type_name_is_an_error_with_the_text() -> None:
    """The class of the variant has its fields."""
    with pytest.raises(gs.AsvoErrorInvalidJobType) as excinfo:
        gs.JobType.parse_name("nonsense")

    assert isinstance(excinfo.value, gs.AsvoError)
    assert excinfo.value.kind == "InvalidJobType"
    assert excinfo.value.str == "nonsense"


def test_an_invalid_option_is_an_error_with_the_name_of_the_option(
    host: str, mock_login: None, tmp_path: object
) -> None:
    """A download option that cannot be used is `AsvoErrorInvalidOption` and not `ValueError`."""
    client = gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))
    options = gs.DownloadOptions(str(tmp_path), buffer_size=0)

    with pytest.raises(gs.AsvoErrorInvalidOption) as excinfo:
        client.download_job(1, options)

    assert not isinstance(excinfo.value, ValueError)
    assert excinfo.value.kind == "InvalidOption"
    assert excinfo.value.name == "buffer_size"
    assert "at least" in excinfo.value.message


def test_a_zero_api_timeout_is_an_invalid_option(host: str) -> None:
    """The client refuses a timeout of 0 with the library's error, before any request."""
    with pytest.raises(gs.AsvoErrorInvalidOption) as excinfo:
        gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY, api_timeout=0.0))

    assert excinfo.value.name == "api_timeout"


def test_an_api_error_in_a_check_is_wrapped_as_the_library_does(host: str, httpserver: HTTPServer) -> None:
    """`AsvoErrorAsvoApi` has the `AsvoApiError` as its `source` and its `__cause__`, and the same message."""
    httpserver.expect_request(LOGIN_PATH, method="POST").respond_with_data("invalid api key", status=401)

    with pytest.raises(gs.AsvoErrorAsvoApi) as excinfo:
        gs.AsvoClient(gs.AsvoClientConfig(host, TEST_API_KEY))

    error = excinfo.value
    assert isinstance(error, gs.AsvoError)
    assert not isinstance(error, gs.AsvoApiError)
    assert error.kind == "AsvoApi"
    assert isinstance(error.source, gs.AsvoApiErrorAuthenticationFailed)
    assert error.__cause__ is error.source
    assert str(error) == str(error.source)
