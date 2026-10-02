"""Tests for the mwa_giant_squid module itself: import, version and logging."""

import importlib.metadata
from datetime import datetime, timezone
from pathlib import Path
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


def _kind(error: pytest.ExceptionInfo[ValueError]) -> str:
    """The ``kind`` attribute of a parse error."""
    return error.value.kind  # ty: ignore[unresolved-attribute]


def test_obs_ids_only_returns_the_obsids_in_order() -> None:
    """``parse_obs_ids_only`` takes obsids, in the order given."""
    assert mwa_giant_squid.parse_obs_ids_only(["1065880248", "1065880128"]) == [1065880248, 1065880128]


def test_obs_ids_only_refuses_a_job_id_even_with_obsids() -> None:
    """A job ID is an error that names it, and has the kind ``JobIdsGiven``."""
    with pytest.raises(ValueError, match=r"^Expected only obsids, but found these job IDs: \[31\]$") as error:
        mwa_giant_squid.parse_obs_ids_only(["1065880128", "31"])

    assert _kind(error) == "JobIdsGiven"


def test_obs_ids_only_refuses_no_obsid() -> None:
    """Nothing at all is an error of kind ``NoObsIds``."""
    with pytest.raises(ValueError, match=r"^No obsids specified!$") as error:
        mwa_giant_squid.parse_obs_ids_only([])

    assert _kind(error) == "NoObsIds"


def test_job_ids_only_returns_the_job_ids_in_order() -> None:
    """``parse_job_ids_only`` takes job IDs, in the order given."""
    assert mwa_giant_squid.parse_job_ids_only(["31", "7"]) == [31, 7]


def test_job_ids_only_refuses_every_obsid_with_a_hint() -> None:
    """An obsid is an error of kind ``ObsIdsGiven`` that names all of them and says how to find the job IDs."""
    with pytest.raises(ValueError, match=r"found these obsids: 1065880128, 1065880248\. To find the job IDs") as error:
        mwa_giant_squid.parse_job_ids_only(["1065880128", "31", "1065880248"])

    assert _kind(error) == "ObsIdsGiven"


def test_job_ids_only_refuses_no_job_id() -> None:
    """Nothing at all is an error of kind ``NoJobIds``."""
    with pytest.raises(ValueError, match=r"^No jobids specified!$") as error:
        mwa_giant_squid.parse_job_ids_only([])

    assert _kind(error) == "NoJobIds"


def test_the_guards_read_files_like_the_other_parser(tmp_path: Path) -> None:
    """A file of IDs works, a file that is missing is an ``OSError``, and bad text in a file is a ``ValueError``."""
    ids = tmp_path / "ids.txt"
    ids.write_text("1065880128\n1065880248\n")
    bad = tmp_path / "bad.txt"
    bad.write_text("1065880128 nonsense\n")

    assert mwa_giant_squid.parse_obs_ids_only([str(ids)]) == [1065880128, 1065880248]
    with pytest.raises(FileNotFoundError):
        mwa_giant_squid.parse_job_ids_only([str(tmp_path / "missing.txt")])
    with pytest.raises(ValueError, match="could not be parsed as an int") as error:
        mwa_giant_squid.parse_obs_ids_only([str(bad)])
    assert _kind(error) == "InsideFile"


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("2026-09-01", datetime(2026, 9, 1, tzinfo=timezone.utc)),
        ("2026-09-01T00:00:00Z", datetime(2026, 9, 1, tzinfo=timezone.utc)),
        ("2026-09-01T08:00:00+08:00", datetime(2026, 9, 1, tzinfo=timezone.utc)),
    ],
)
def test_parse_utc_time_reads_a_date_or_rfc_3339(text: str, expected: datetime) -> None:
    """A date is midnight UTC; an RFC 3339 time is the same instant with its offset applied."""
    assert mwa_giant_squid.parse_utc_time(text) == expected


@pytest.mark.parametrize("text", ["2026-09-01T10:00:00", "2026-9-1x", "yesterday", ""])
def test_parse_utc_time_refuses_a_time_without_an_offset(text: str) -> None:
    """Any other text is a ``ValueError`` of kind ``InvalidTime``, and the message says what is accepted."""
    with pytest.raises(ValueError, match=r"^not a time: use RFC 3339 \(2026-09-01T00:00:00Z\) or a date") as error:
        mwa_giant_squid.parse_utc_time(text)

    assert _kind(error) == "InvalidTime"
