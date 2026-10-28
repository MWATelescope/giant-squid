"""Tests for `AsvoClientConfig`, `DownloadSettings` and the functions that read them from the environment."""

import pathlib

import pytest

import mwa_giant_squid as gs

from .conftest import TEST_API_KEY

HOST = "https://asvo.example.org:443"
CUSTOM_TIMEOUT = 12.5
ENV_TIMEOUT_SECONDS = 45
ENV_BUFFER_MIB = 7
ENV_RETRY_SECONDS = 33
BYTES_PER_MIB = 1024 * 1024

# The variables that the environment functions read. Each test starts without them.
ALL_ENV = [
    gs.ENV_MWA_ASVO_HOST,
    gs.ENV_MWA_ASVO_API_KEY,
    gs.ENV_MWA_ASVO_API_TIMEOUT,
    gs.ENV_HOME,
    gs.ENV_GIANT_SQUID_BUF_SIZE,
    gs.ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS,
]


@pytest.fixture(autouse=True)
def clean_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """No variable that the library reads is set."""
    for name in ALL_ENV:
        monkeypatch.delenv(name, raising=False)


def test_a_config_has_the_defaults_of_the_library() -> None:
    """The timeout is the library's, and nothing is cached."""
    config = gs.AsvoClientConfig(HOST, TEST_API_KEY)

    assert config.host == HOST
    assert config.api_key == TEST_API_KEY
    assert config.api_timeout == gs.DEFAULT_API_TIMEOUT
    assert config.token_cache_path is None


def test_a_config_takes_a_timeout_and_a_cache_path(tmp_path: pathlib.Path) -> None:
    """Both are keyword-only. A cache path comes back as a `Path`."""
    config = gs.AsvoClientConfig(HOST, TEST_API_KEY, api_timeout=CUSTOM_TIMEOUT, token_cache_path=str(tmp_path))

    assert config.api_timeout == CUSTOM_TIMEOUT
    assert config.token_cache_path == tmp_path
    assert isinstance(config.token_cache_path, pathlib.Path)
    with pytest.raises(TypeError):
        gs.AsvoClientConfig(HOST, TEST_API_KEY, CUSTOM_TIMEOUT)  # ty: ignore[too-many-positional-arguments]


def test_a_config_can_be_changed() -> None:
    """The attributes are the fields of the Rust struct, and they can be set."""
    config = gs.AsvoClientConfig(HOST, TEST_API_KEY)

    config.host = "http://127.0.0.1:1"
    config.api_timeout = CUSTOM_TIMEOUT
    config.token_cache_path = pathlib.Path("tokens.json")

    assert config.host == "http://127.0.0.1:1"
    assert config.api_timeout == CUSTOM_TIMEOUT
    assert config.token_cache_path == pathlib.Path("tokens.json")


def test_the_repr_does_not_show_the_api_key() -> None:
    """A repr goes to logs and tracebacks."""
    text = repr(gs.AsvoClientConfig(HOST, TEST_API_KEY))

    assert TEST_API_KEY not in text
    assert HOST in text


@pytest.mark.parametrize("timeout", [-1.0, float("nan"), float("inf")])
def test_a_client_refuses_a_timeout_that_is_not_valid(timeout: float) -> None:
    """The timeout is checked when the client is made, and nothing is sent."""
    with pytest.raises(ValueError, match="api_timeout"):
        gs.AsvoClient(gs.AsvoClientConfig(HOST, TEST_API_KEY, api_timeout=timeout))


def test_the_client_config_from_the_environment(monkeypatch: pytest.MonkeyPatch, tmp_path: pathlib.Path) -> None:
    """The key, the host, the timeout and the cache path come from the variables."""
    monkeypatch.setenv(gs.ENV_MWA_ASVO_API_KEY, TEST_API_KEY)
    monkeypatch.setenv(gs.ENV_MWA_ASVO_HOST, HOST)
    monkeypatch.setenv(gs.ENV_MWA_ASVO_API_TIMEOUT, str(ENV_TIMEOUT_SECONDS))
    monkeypatch.setenv(gs.ENV_HOME, str(tmp_path))

    config = gs.client_config_from_env()

    assert isinstance(config, gs.AsvoClientConfig)
    assert config.api_key == TEST_API_KEY
    assert config.host == HOST
    assert config.api_timeout == ENV_TIMEOUT_SECONDS
    assert config.token_cache_path == gs.default_token_cache_path(tmp_path)


def test_the_client_config_from_the_environment_has_defaults(monkeypatch: pytest.MonkeyPatch) -> None:
    """Only the key is needed. Without `HOME`, the session is not cached."""
    monkeypatch.setenv(gs.ENV_MWA_ASVO_API_KEY, TEST_API_KEY)

    config = gs.client_config_from_env()

    assert config.host == gs.DEFAULT_ASVO_HOST
    assert config.api_timeout == gs.DEFAULT_API_TIMEOUT
    assert config.token_cache_path is None


def test_the_client_config_from_the_environment_needs_a_key() -> None:
    """No key is an `AsvoApiError`, which names the variable."""
    with pytest.raises(gs.AsvoApiError, match=gs.ENV_MWA_ASVO_API_KEY) as excinfo:
        gs.client_config_from_env()
    assert excinfo.value.kind == "MissingAuthKey"


@pytest.mark.parametrize("value", ["soon", "1.5", "0", "-3"])
def test_the_client_config_from_the_environment_refuses_a_bad_timeout(
    monkeypatch: pytest.MonkeyPatch, value: str
) -> None:
    """The timeout is a whole number of seconds, at least 1."""
    monkeypatch.setenv(gs.ENV_MWA_ASVO_API_KEY, TEST_API_KEY)
    monkeypatch.setenv(gs.ENV_MWA_ASVO_API_TIMEOUT, value)

    with pytest.raises(gs.AsvoError, match=gs.ENV_MWA_ASVO_API_TIMEOUT) as excinfo:
        gs.client_config_from_env()
    assert excinfo.value.kind == "InvalidEnvironment"


def test_download_settings_have_the_defaults_of_the_library() -> None:
    """Without arguments, the settings are the defaults."""
    settings = gs.DownloadSettings()

    assert settings.buffer_size == gs.DEFAULT_DOWNLOAD_BUFFER_SIZE
    assert settings.retry_duration == gs.DEFAULT_DOWNLOAD_RETRY_DURATION


def test_download_settings_take_values() -> None:
    """The retry duration is in seconds."""
    settings = gs.DownloadSettings(buffer_size=ENV_BUFFER_MIB, retry_duration=CUSTOM_TIMEOUT)

    assert settings.buffer_size == ENV_BUFFER_MIB
    assert settings.retry_duration == CUSTOM_TIMEOUT


def test_download_settings_from_the_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """The buffer size is a number of MiB in the variable, and bytes in the settings."""
    monkeypatch.setenv(gs.ENV_GIANT_SQUID_BUF_SIZE, str(ENV_BUFFER_MIB))
    monkeypatch.setenv(gs.ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS, str(ENV_RETRY_SECONDS))

    settings = gs.DownloadSettings.from_env()

    assert settings.buffer_size == ENV_BUFFER_MIB * BYTES_PER_MIB
    assert settings.retry_duration == ENV_RETRY_SECONDS


def test_download_settings_from_an_empty_environment() -> None:
    """Every variable is optional."""
    settings = gs.DownloadSettings.from_env()

    assert settings.buffer_size == gs.DEFAULT_DOWNLOAD_BUFFER_SIZE
    assert settings.retry_duration == gs.DEFAULT_DOWNLOAD_RETRY_DURATION


@pytest.mark.parametrize("name", [gs.ENV_GIANT_SQUID_BUF_SIZE, gs.ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS])
def test_download_settings_refuse_a_value_that_is_not_a_whole_number(
    monkeypatch: pytest.MonkeyPatch, name: str
) -> None:
    """The error is of kind `InvalidEnvironment`, and names the variable."""
    monkeypatch.setenv(name, "lots")

    with pytest.raises(gs.AsvoError, match=name) as excinfo:
        gs.DownloadSettings.from_env()
    assert excinfo.value.kind == "InvalidEnvironment"
