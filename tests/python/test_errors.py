"""Tests for the exception classes of `MwaAsvoApiError` and `MwaAsvoError`: a subclass for each Rust variant."""

import pytest
from pytest_httpserver import HTTPServer

import mwa_giant_squid as gs

from .conftest import LOGIN_PATH, TEST_API_KEY, TEST_OBS_ID
from .parity_manifest import API_ERROR_VARIANTS, ERROR_VARIANTS, EXCLUDED_ERROR_VARIANTS

# The variants that have a class: the others raise OSError.
ERROR_VARIANTS_WITH_A_CLASS = [v for v in ERROR_VARIANTS if v not in EXCLUDED_ERROR_VARIANTS]


@pytest.mark.parametrize("variant", API_ERROR_VARIANTS)
def test_each_api_error_variant_has_a_subclass(variant: str) -> None:
    """The class has the names of the base class and the variant, and is an `MwaAsvoApiError`."""
    cls = getattr(gs, f"MwaAsvoApiError{variant}")

    assert issubclass(cls, gs.MwaAsvoApiError)
    assert not issubclass(cls, ValueError)
    assert cls.__module__ == "mwa_giant_squid"


@pytest.mark.parametrize("variant", ERROR_VARIANTS_WITH_A_CLASS)
def test_each_error_variant_has_a_subclass(variant: str) -> None:
    """The class has the names of the base class and the variant, and is an `MwaAsvoError`."""
    cls = getattr(gs, f"MwaAsvoError{variant}")

    assert issubclass(cls, gs.MwaAsvoError)
    assert not issubclass(cls, ValueError)
    assert cls.__module__ == "mwa_giant_squid"


@pytest.mark.parametrize("variant", EXCLUDED_ERROR_VARIANTS)
def test_a_variant_that_python_raises_as_os_error_has_no_class(variant: str) -> None:
    """The I/O variants are `OSError`, with its subclasses."""
    assert not hasattr(gs, f"MwaAsvoError{variant}")


def test_the_base_classes_are_exceptions() -> None:
    """The two bases are plain exceptions, and not each other's subclass."""
    assert issubclass(gs.MwaAsvoApiError, Exception)
    assert issubclass(gs.MwaAsvoError, Exception)
    assert not issubclass(gs.MwaAsvoError, gs.MwaAsvoApiError)
    assert not issubclass(gs.MwaAsvoApiError, gs.MwaAsvoError)


def test_an_invalid_parameter_is_an_api_error_that_is_not_a_value_error() -> None:
    """A job argument outside the schema limits is an `MwaAsvoApiError` of kind `InvalidParameter`."""
    with pytest.raises(gs.MwaAsvoApiErrorInvalidParameter) as excinfo:
        gs.JobsFilter(job_ids=[1], obs_ids=[TEST_OBS_ID]).validate()

    error = excinfo.value
    assert isinstance(error, gs.MwaAsvoApiError)
    assert not isinstance(error, ValueError)
    assert error.kind == "InvalidParameter"
    assert error.name == "job_ids"
    assert "Job IDs and Obs IDs" in error.message
    assert str(error) == f"Invalid {error.name}: {error.message}"


def test_an_api_error_in_a_check_is_wrapped_as_the_library_does(host: str, httpserver: HTTPServer) -> None:
    """`MwaAsvoErrorApi` has the `MwaAsvoApiError` as its `source` and its `__cause__`, and the same message."""
    httpserver.expect_request(LOGIN_PATH, method="POST").respond_with_data("invalid api key", status=401)

    with pytest.raises(gs.MwaAsvoErrorApi) as excinfo:
        gs.MwaAsvoClient(gs.MwaAsvoClientConfig(host, TEST_API_KEY))

    error = excinfo.value
    assert isinstance(error, gs.MwaAsvoError)
    assert not isinstance(error, gs.MwaAsvoApiError)
    assert error.kind == "Api"
    assert isinstance(error.source, gs.MwaAsvoApiErrorAuthenticationFailed)
    assert error.__cause__ is error.source
    assert str(error) == str(error.source)
