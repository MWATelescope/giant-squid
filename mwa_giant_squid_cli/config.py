# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Build the client and the download settings from the environment.

The ``mwa_giant_squid`` module reads no environment variables. This program reads them, as the Rust
``giant-squid`` command does, and gives the module explicit values.
"""

import logging
import os
from dataclasses import dataclass
from pathlib import Path

from mwa_giant_squid import AsvoClient

from .constants import (
    BYTES_PER_MIB,
    DEFAULT_HOST,
    ENV_API_KEY,
    ENV_API_TIMEOUT,
    ENV_BUF_SIZE,
    ENV_DOWNLOAD_RETRY_SECS,
    ENV_HOME,
    ENV_HOST,
    TOKEN_CACHE_PARTS,
)

log = logging.getLogger(__name__)


@dataclass(frozen=True)
class DownloadSettings:
    """The download settings that come from the environment.

    Attributes:
        buffer_size: How many bytes to hold in memory before they are written, or ``None`` for the library
            default.
        retry_duration: How long to retry a failing download, in seconds, or ``None`` for the library
            default.
    """

    buffer_size: int | None
    retry_duration: float | None


def api_timeout_from_env() -> float | None:
    """Read the API request timeout.

    Returns:
        The timeout in seconds from ``MWA_ASVO_API_TIMEOUT``, or ``None`` (the library default) if the variable
        is not set or is not a whole number of seconds.
    """
    value = os.environ.get(ENV_API_TIMEOUT)
    if value is None:
        return None
    try:
        return float(int(value))
    except ValueError:
        log.warning(
            "Environment variable %s='%s' is not valid, using the default. (It should be an integer number of seconds)",
            ENV_API_TIMEOUT,
            value,
        )
        return None


def token_cache_path_from_env() -> Path | None:
    """Find where to cache the session.

    Returns:
        The token file under the home directory, which ``giant-squid`` and ``manta-ray-client`` share, or ``None``
        if ``HOME`` is not set (then the session is not cached).
    """
    home = os.environ.get(ENV_HOME)
    if not home:
        log.debug("%s is not set; the MWA ASVO session will not be cached", ENV_HOME)
        return None
    return Path(home).joinpath(*TOKEN_CACHE_PARTS)


def connect() -> AsvoClient:
    """Log in to the MWA ASVO with the settings from the environment.

    Returns:
        A client that is logged in.

    Raises:
        AsvoApiError: ``MWA_ASVO_API_KEY`` is not set (or is empty), or the login failed.
    """
    return AsvoClient(
        os.environ.get(ENV_HOST, DEFAULT_HOST),
        os.environ.get(ENV_API_KEY, ""),
        api_timeout=api_timeout_from_env(),
        token_cache_path=token_cache_path_from_env(),
    )


def download_settings_from_env() -> DownloadSettings:
    """Read the download buffer size and retry duration.

    Returns:
        The settings. A setting that is not set is ``None``, so the library default applies.

    Raises:
        ValueError: ``GIANT_SQUID_BUF_SIZE`` is not a whole number of MiB.
    """
    buffer_size = None
    value = os.environ.get(ENV_BUF_SIZE)
    if value is not None:
        try:
            buffer_size = int(value) * BYTES_PER_MIB
        except ValueError:
            msg = f"Environment variable {ENV_BUF_SIZE}='{value}' is not valid. (It should be an integer number of MiB)"
            raise ValueError(msg) from None
        if buffer_size < 0:
            msg = f"Environment variable {ENV_BUF_SIZE}='{value}' is not valid. (It should not be negative)"
            raise ValueError(msg)

    retry_duration = None
    value = os.environ.get(ENV_DOWNLOAD_RETRY_SECS)
    if value is not None:
        try:
            retry_duration = float(int(value))
        except ValueError:
            log.debug("%s='%s' is not a whole number of seconds; using the default", ENV_DOWNLOAD_RETRY_SECS, value)
    return DownloadSettings(buffer_size, retry_duration)
