"""Shared fixtures: a local mock MWA ASVO, so the tests never contact a real server."""

import base64
import json
import time
from collections.abc import Callable
from typing import Any

import pytest
from pytest_httpserver import HTTPServer

# Values the mock uses in place of real credentials and user details.
TEST_API_KEY = "not-a-real-api-key"
TEST_USER_ID = 4242
TEST_USER_LOGIN = "test_user"
TEST_USER_EMAIL = "test_user@example.org"

# An obsid used across the tests.
TEST_OBSID = 1065880128

# The MWA ASVO endpoints the mock serves.
LOGIN_PATH = "/api/v2/api_login"
GET_JOBS_PATH = "/api/v2/get_jobs"

# Token lifetimes, in seconds.
ACCESS_TOKEN_LIFETIME = 3600
REFRESH_TOKEN_LIFETIME = 86400

# The server's job_type number for a visibility download.
JOB_TYPE_DOWNLOAD_VISIBILITIES = 1


def jwt_expiring_in(seconds: int) -> str:
    """A JWT whose payload has an ``exp`` claim ``seconds`` from now.

    The client only decodes the payload to read ``exp``; it never checks the signature, so the header and
    signature are placeholders.

    Args:
        seconds: The token lifetime.

    Returns:
        The token.
    """
    claims = json.dumps({"exp": int(time.time()) + seconds}).encode()
    payload = base64.urlsafe_b64encode(claims).decode().rstrip("=")
    return f"notaheader.{payload}.notasignature"


def login_response() -> dict[str, Any]:
    """A successful ``ApiLoginResponse`` body.

    Returns:
        The response body.
    """
    return {
        "access_token": jwt_expiring_in(ACCESS_TOKEN_LIFETIME),
        "refresh_token": jwt_expiring_in(REFRESH_TOKEN_LIFETIME),
        "token_type": "bearer",
        "user": {"email": TEST_USER_EMAIL, "id": TEST_USER_ID, "is_superuser": False, "login": TEST_USER_LOGIN},
    }


def job_detail(job_id: int, job_state: str, **extra: Any) -> dict[str, Any]:
    """One entry of a ``get_jobs`` page, as the real server sends it.

    Args:
        job_id: The job ID.
        job_state: The server's state name, for example "queued" or "completed".
        **extra: More fields, for example ``error_text`` or ``product``.

    Returns:
        The job.
    """
    job = {
        "created": "2026-09-08T05:41:54.757232",
        "first_name": "Test",
        "id": job_id,
        "job_params": {"obs_id": str(TEST_OBSID), "delivery": "acacia"},
        "job_state": job_state,
        "job_type": JOB_TYPE_DOWNLOAD_VISIBILITIES,
        "last_name": "User",
        "user_id": TEST_USER_ID,
    }
    job.update(extra)
    return job


def error_response(error_code: str, message: str) -> dict[str, Any]:
    """A structured ``ErrorResponse`` body.

    Args:
        error_code: The machine-readable error code.
        message: The message.

    Returns:
        The response body.
    """
    return {"error_code": error_code, "message": message, "detail": "detail from the mock", "suggestion": "try again"}


@pytest.fixture
def host(httpserver: HTTPServer) -> str:
    """The mock server's base URL, as the client's host (no trailing slash).

    Args:
        httpserver: The pytest-httpserver server.

    Returns:
        The URL.
    """
    return f"http://{httpserver.host}:{httpserver.port}"


@pytest.fixture
def mock_login(httpserver: HTTPServer) -> None:
    """Serve a successful login.

    Args:
        httpserver: The pytest-httpserver server.
    """
    httpserver.expect_request(LOGIN_PATH, method="POST").respond_with_json(login_response())


@pytest.fixture
def serve_jobs(httpserver: HTTPServer) -> Callable[[list[dict[str, Any]]], None]:
    """A function that serves one page of jobs from ``get_jobs``.

    Args:
        httpserver: The pytest-httpserver server.

    Returns:
        The function.
    """

    def serve(jobs: list[dict[str, Any]]) -> None:
        httpserver.expect_request(GET_JOBS_PATH, method="POST").respond_with_json(
            {"jobs": jobs, "total_count": len(jobs)}
        )

    return serve
