"""The manifest of the Python API against the Rust API.

Each ``Entry`` names one public item of the Rust library and says what the Python module does with it. The test in
``test_parity.py`` checks the manifest against the real module and against ``src/lib.rs``. The goal is that the Python
API matches the Rust API where possible (see ``docs/PYTHON_BINDINGS.md``).

The status of an entry:

``done``
    The Python module has the item. ``python`` is its dotted name in ``mwa_giant_squid``.
``pending``
    The Python module must get the item, but does not have it yet. ``python`` is the name that it will have. Change
    the status to ``done`` in the step that adds it (the test fails when a ``pending`` item already exists).
``excluded``
    The Python module does not have the item, on purpose. ``note`` says why.
``undecided``
    Not decided yet. ``note`` says what the question is.

A Rust path is ``crate::Name`` for an item that ``src/lib.rs`` re-exports at the crate root, ``crate::Type::member``
for a member of such an item, and the module path (``mwa_asvo::api::job_params::download_params``) for any other item.
"""

from dataclasses import dataclass
from typing import Literal

Status = Literal["done", "pending", "excluded", "undecided"]

# The variants of the two Rust error enums (``src/mwa_asvo/api/error.rs`` and ``src/mwa_asvo/error.rs``). Each variant
# is a Python subclass of the enum's exception, named by the enum and the variant (``AsvoApiErrorInvalidParameter``).
API_ERROR_VARIANTS = [
    "MissingAuthKey",
    "AuthenticationFailed",
    "Conversion",
    "InvalidParameter",
    "BadJson",
    "Reqwest",
    "ApiError",
    "BadStatus",
]
ERROR_VARIANTS = [
    "NoAsvoJob",
    "JobFailed",
    "InvalidJob",
    "JobCancelled",
    "NoObsId",
    "NoJobReadyForObsId",
    "TooManyObsIds",
    "NotReady",
    "NoFiles",
    "HashMismatch",
    "InvalidJobState",
    "InvalidJobType",
    "InvalidEnvironment",
    "InvalidOption",
    "AsvoApi",
    "IO",
    "Interrupted",
    "NoUrl",
    "NoHash",
    "FilesNotReachable",
    "NoPath",
    "SymlinkInDownloadDir",
    "DestinationExists",
    "HttpError",
    "CannotCreate",
    "JobExpired",
    "Http404Error",
]

# The variants of `AsvoError` that have no subclass, because Python raises another exception for them.
EXCLUDED_ERROR_VARIANTS = {
    "IO": "Python raises OSError (FileNotFoundError, PermissionError and so on), not an AsvoError.",
}

# The variants of `ObsIdError` (`src/obs_id/mod.rs`) and `ParseError` (`src/helpers/mod.rs`).
OBS_ID_ERROR_VARIANTS = ["WrongNumDigits", "Parse"]
PARSE_ERROR_VARIANTS = [
    "InsideFile",
    "InsideStdin",
    "StdinIO",
    "StdinError",
    "StdinJson",
    "IO",
    "JobIdsGiven",
    "ObsIdsGiven",
    "ZeroJobId",
    "NoObsIds",
    "NoJobIds",
    "InvalidTime",
]
# The variants of `ParseError` that have no subclass, because Python raises another exception for them.
PARSE_ERROR_EXCLUDED = {
    "StdinIO": "Python raises OSError, not a ParseError.",
    "IO": "Python raises OSError (FileNotFoundError, PermissionError and so on), not a ParseError.",
}

# The names in the Python module that the Rust library does not have, and why.
PYTHON_ONLY = {
    "reset_logging": "Connects Python logging to the Rust `log` records (pyo3-log). Rust code has its own logger.",
}


@dataclass(frozen=True)
class Entry:
    """One Rust public item and what the Python module does with it."""

    rust: str
    status: Status
    python: str | None = None
    note: str = ""


def done(rust: str, python: str) -> Entry:
    """An item that the Python module has."""
    return Entry(rust, "done", python)


def pending(rust: str, python: str, note: str = "") -> Entry:
    """An item that the Python module must get."""
    return Entry(rust, "pending", python, note)


def excluded(rust: str, note: str) -> Entry:
    """An item that the Python module does not have, on purpose."""
    return Entry(rust, "excluded", None, note)


def undecided(rust: str, note: str, python: str | None = None) -> Entry:
    """An item that is not decided yet."""
    return Entry(rust, "undecided", python, note)


def _variant_entries(enum: str, variants: list[str], excluded_variants: dict[str, str] | None = None) -> list[Entry]:
    """The entries of the variants of a Rust error enum: each is a Python subclass of the enum's exception."""
    excluded_variants = excluded_variants or {}
    entries: list[Entry] = []
    for variant in variants:
        rust = f"crate::{enum}::{variant}"
        if variant in excluded_variants:
            entries.append(excluded(rust, excluded_variants[variant]))
        else:
            entries.append(done(rust, f"{enum}{variant}"))
    return entries


def _errors() -> list[Entry]:
    entries: list[Entry] = []
    for variant in API_ERROR_VARIANTS:
        entries.append(pending(f"crate::AsvoApiError::{variant}", f"AsvoApiError{variant}"))
    for variant in ERROR_VARIANTS:
        rust = f"crate::AsvoError::{variant}"
        if variant in EXCLUDED_ERROR_VARIANTS:
            entries.append(excluded(rust, EXCLUDED_ERROR_VARIANTS[variant]))
        else:
            entries.append(pending(rust, f"AsvoError{variant}"))
    return entries


ENV_CONSTANTS = [
    "ENV_GIANT_SQUID_BUF_SIZE",
    "ENV_GIANT_SQUID_DELIVERY",
    "ENV_GIANT_SQUID_DELIVERY_FORMAT",
    "ENV_GIANT_SQUID_DOWNLOAD_RETRY_SECS",
    "ENV_HOME",
    "ENV_MWA_ASVO_API_KEY",
    "ENV_MWA_ASVO_API_TIMEOUT",
    "ENV_MWA_ASVO_HOST",
]

SCHEMA_TYPES = [
    "ConversionJobParams",
    "DeliveryDestination",
    "DeliveryFormat",
    "DownloadJobParams",
    "ImageOutputMode",
    "ImageWeighting",
    "ImagingJobFlow1Params",
    "ImagingJobFlow2Params",
    "JobCancelledResponse",
    "JobFile",
    "JobProduct",
    "JobState",
    "JobSubmittedResponse",
    "JobType",
    "OutputFormat",
    "PhaseCentre",
    "Polarization",
    "SortBy",
    "Status",
    "Type",
]

ASVO_JOB_MEMBERS = [
    "job_id",
    "obs_id",
    "job_type",
    "job_state",
    "error_code",
    "error_text",
    "product",
    "completed",
    "created",
    "started",
    "modified",
    "user_id",
    "first_name",
    "last_name",
    "job_params",
    "expires",
    "is_expired",
    "display_state",
]

# Why the helpers of the Rust validation module have no Python counterpart.
VALIDATION_NOTE = (
    "Python validation is the Rust validation: the bindings call it, and the `validate()` methods and the submit "
    "methods run it."
)

MANIFEST: list[Entry] = [
    # --- The client ---
    done("crate::AsvoClient", "AsvoClient"),
    done("crate::AsvoClient::new", "AsvoClient"),
    done("crate::AsvoClient::get_jobs", "AsvoClient.get_jobs"),
    done("crate::AsvoClient::submit_download_job", "AsvoClient.submit_download_job"),
    done("crate::AsvoClient::submit_preprocess_job", "AsvoClient.submit_preprocess_job"),
    done("crate::AsvoClient::submit_csv_file", "AsvoClient.submit_csv_file"),
    done("crate::AsvoClient::cancel_job", "AsvoClient.cancel_job"),
    done("crate::AsvoClient::download_job", "AsvoClient.download_job"),
    done("crate::AsvoClient::download_obs", "AsvoClient.download_obs"),
    pending("crate::AsvoClient::download_job_from", "AsvoClient.download_job_from", "Replaces the `jobs=` argument."),
    pending("crate::AsvoClient::download_obs_from", "AsvoClient.download_obs_from", "Replaces the `jobs=` argument."),
    pending("crate::AsvoClient::submit_csv_jobs", "AsvoClient.submit_csv_jobs"),
    pending("crate::AsvoClient::submit_csv_job", "AsvoClient.submit_csv_job"),
    pending(
        "crate::AsvoClientConfig",
        "AsvoClientConfig",
        "AsvoClient takes it, instead of host, api_key, api_timeout and token_cache_path.",
    ),
    pending(
        "crate::JobsFilter",
        "JobsFilter",
        "AsvoClient.get_jobs takes it, instead of the keyword arguments.",
    ),
    pending("crate::JobsFilter::validate", "JobsFilter.validate"),
    pending(
        "crate::DownloadOptions",
        "DownloadOptions",
        "The download methods take it, instead of the keyword arguments. `download_dir` is a field.",
    ),
    excluded(
        "crate::DownloadOptions::should_stop",
        "Python stops a download with Ctrl-C (KeyboardInterrupt). The library reads no signals.",
    ),
    excluded("crate::API_PREFIX", "An internal detail of the HTTP client."),
    excluded("crate::ENDPOINT_JOBS", "An internal detail of the HTTP client."),
    excluded("crate::ENDPOINT_DOWNLOAD_JOB", "An internal detail of the HTTP client."),
    excluded("crate::ENDPOINT_PREPROCESS_JOB", "An internal detail of the HTTP client."),
    done("crate::DEFAULT_ASVO_HOST", "DEFAULT_ASVO_HOST"),
    done("crate::DEFAULT_API_TIMEOUT", "DEFAULT_API_TIMEOUT"),  # Seconds, as a float (a Rust `Duration`).
    # --- Downloads ---
    done("crate::DownloadProgress", "DownloadProgress"),
    done("crate::DownloadProgress::Started", "DownloadProgress.Started"),
    done("crate::DownloadProgress::Advanced", "DownloadProgress.Advanced"),
    done("crate::DownloadProgress::Finished", "DownloadProgress.Finished"),
    done("crate::BYTES_PER_MIB", "BYTES_PER_MIB"),
    done("crate::MIN_DOWNLOAD_BUFFER_SIZE", "MIN_DOWNLOAD_BUFFER_SIZE"),
    done("crate::DEFAULT_DOWNLOAD_BUFFER_SIZE", "DEFAULT_DOWNLOAD_BUFFER_SIZE"),
    done("crate::DEFAULT_DOWNLOAD_RETRY_DURATION", "DEFAULT_DOWNLOAD_RETRY_DURATION"),  # Seconds, as a float.
    pending("crate::DownloadSettings", "DownloadSettings"),
    pending("crate::DownloadSettings::from_env", "DownloadSettings.from_env"),
    # --- The environment and the token cache ---
    pending("crate::client_config_from_env", "client_config_from_env"),
    *[done(f"crate::{name}", name) for name in ENV_CONSTANTS],
    done("crate::default_token_cache_path", "default_token_cache_path"),
    excluded("crate::StoredTokens", "An internal detail of the token cache. Python gives a path only."),
    # --- Jobs ---
    done("crate::AsvoJob", "AsvoJob"),
    *[done(f"crate::AsvoJob::{name}", f"AsvoJob.{name}") for name in ASVO_JOB_MEMBERS],
    done("crate::AsvoJob::detail", "AsvoJob.detail"),
    done("mwa_asvo::api::openapi::JobDetailResponse", "JobDetailResponse"),
    excluded("crate::AsvoJob::is_expired_at", "A clock-injection helper for tests."),
    excluded("crate::AsvoJob::display_state_at", "A clock-injection helper for tests."),
    done("crate::AsvoJobVec", "AsvoJobVec"),
    done("crate::AsvoJobVec::filter", "AsvoJobVec.filter"),
    done("crate::AsvoJobVec::all_ready", "AsvoJobVec.all_ready"),
    done("crate::AsvoJobVec::json", "AsvoJobVec.json"),
    excluded("crate::AsvoJobVec::all_ready_at", "A clock-injection helper for tests."),
    excluded("crate::AsvoJobVec::retain", "Takes a Rust closure. `filter` and Python list comprehensions cover it."),
    excluded("crate::AsvoJobId", "An alias of a non-zero u64. Python uses `int`."),
    done("crate::SIGNED_URL_LIFETIME_DAYS", "SIGNED_URL_LIFETIME_DAYS"),
    done("mwa_asvo::types::JobType::names", "JobType.names"),
    done("mwa_asvo::types::JobType::parse_name", "JobType.parse_name"),
    done("mwa_asvo::types::JobState::names", "JobState.names"),
    done("mwa_asvo::types::JobState::parse_name", "JobState.parse_name"),
    done("mwa_asvo::types::JobFile::size_bytes", "JobFile.size_bytes"),
    # --- The schema types, which the Python module already has ---
    *[done(f"mwa_asvo::api::openapi::{name}", name) for name in SCHEMA_TYPES],
    # --- Request bodies ---
    pending("mwa_asvo::api::job_params::PreprocessJobParams", "PreprocessJobParams"),
    pending("mwa_asvo::api::job_params::PreprocessJobParams::validate", "PreprocessJobParams.validate"),
    pending("mwa_asvo::api::job_params::download_params", "download_params"),
    pending("mwa_asvo::api::job_params::metadata_params", "metadata_params"),
    pending("mwa_asvo::api::job_params::voltage_params", "voltage_params"),
    pending("mwa_asvo::api::job_params::conversion_params", "conversion_params"),
    pending("mwa_asvo::api::job_params::imaging_params", "imaging_params"),
    pending("mwa_asvo::api::job_params::image_from_job_params", "image_from_job_params"),
    pending("mwa_asvo::api::validate::validate_download_params", "DownloadJobParams.validate"),
    pending("mwa_asvo::api::validate::validate_conversion_params", "ConversionJobParams.validate"),
    pending("mwa_asvo::api::validate::validate_imaging_params", "ImagingJobFlow1Params.validate"),
    pending("mwa_asvo::api::validate::validate_image_from_job_params", "ImagingJobFlow2Params.validate"),
    excluded("mwa_asvo::api::validate::image_size", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::nmiter", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::days", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::check_days", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::source_job_id", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::check_delivery_format", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::check_centre", VALIDATION_NOTE),
    excluded("mwa_asvo::api::validate::Bounds", VALIDATION_NOTE),
    # --- IDs and parsing ---
    done("crate::ObsId", "ObsId"),
    done("crate::ObsId::MIN", "ObsId.MIN"),
    done("crate::ObsId::MAX", "ObsId.MAX"),
    done("crate::ObsId::validate", "ObsId.validate"),
    done("crate::ObsId::get", "ObsId.get"),
    done("crate::ObsId::from_str", "ObsId.from_str"),
    done("crate::ObsIdError", "ObsIdError"),
    done("crate::ParseError", "ParseError"),
    *_variant_entries("ObsIdError", OBS_ID_ERROR_VARIANTS),
    *_variant_entries("ParseError", PARSE_ERROR_VARIANTS, PARSE_ERROR_EXCLUDED),
    done("crate::OBS_ID_HINT", "OBS_ID_HINT"),
    done("crate::parse_obs_ids_only", "parse_obs_ids_only"),
    done("crate::parse_job_ids_only", "parse_job_ids_only"),
    done("crate::parse_many_job_ids_or_obs_ids", "parse_many_job_ids_or_obs_ids"),
    done("crate::parse_job_ids_and_obs_ids_from_file", "parse_job_ids_and_obs_ids_from_file"),
    done("crate::parse_utc_time", "parse_utc_time"),
    done("crate::check_file_sha1_hash", "check_file_sha1_hash"),
    excluded("crate::jiff", "A Rust crate re-export. Python dates are `datetime.datetime`."),
    # --- The manta-ray-client CSV file ---
    done("crate::CsvJobFailure", "CsvJobFailure"),
    done("crate::CsvSubmitError", "CsvSubmitError"),
    pending("crate::CsvDefaults", "CsvDefaults", "AsvoClient.submit_csv_file takes it, instead of keyword arguments."),
    pending("crate::CsvDefaults::from_env", "CsvDefaults.from_env"),
    pending("crate::CsvJob", "CsvJob"),
    pending("crate::CsvJobParams", "CsvJobParams"),
    pending("crate::CsvJobParams::description", "CsvJobParams.description"),
    pending("crate::CsvJobParams::validate", "CsvJobParams.validate"),
    excluded("crate::CsvJobParams::endpoint", "Returns an ENDPOINT_* path, which Python does not have."),
    pending("crate::CsvError", "CsvError"),
    pending("crate::CsvFileError", "CsvFileError"),
    pending("crate::CsvRowError", "CsvRowError"),
    pending("crate::CsvJobsFailed", "CsvJobsFailed"),
    pending("crate::parse_manta_ray_csv", "parse_manta_ray_csv"),
    pending("crate::read_manta_ray_csv", "read_manta_ray_csv"),
    # --- Errors ---
    done("crate::AsvoApiError", "AsvoApiError"),
    done("crate::AsvoError", "AsvoError"),
    *_errors(),
    # --- Other ---
    excluded("crate::stub_info", "Hidden. Only the `stub_gen` binary (maintainers) uses it."),
]
