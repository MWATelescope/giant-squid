"""Tests for the functions that read Job IDs, Obs IDs and times, and the `ParseError` exceptions."""

import datetime
import pathlib

import pytest

import mwa_giant_squid as gs

OBS_ID = 1065880128
OTHER_OBS_ID = 1065880248
JOB_ID = 12345
OTHER_JOB_ID = 678


def write_ids(tmp_path: pathlib.Path, text: str) -> str:
    """A file that has `text`. Its name, as the `str` that the parsers read."""
    path = tmp_path / "ids.txt"
    path.write_text(text)
    return str(path)


def test_the_parse_errors_are_not_value_errors() -> None:
    """The hierarchy is the library's: `ParseError` is an `Exception`, and a variant is a `ParseError`."""
    assert issubclass(gs.ParseError, Exception)
    assert not issubclass(gs.ParseError, ValueError)
    for name in (
        "InsideFile",
        "InsideStdin",
        "StdinError",
        "StdinJson",
        "JobIdsGiven",
        "ObsIdsGiven",
        "ZeroJobId",
        "NoObsIds",
        "NoJobIds",
        "InvalidTime",
    ):
        assert issubclass(getattr(gs, f"ParseError{name}"), gs.ParseError), name


def test_parse_obs_ids_only_returns_obs_ids() -> None:
    """The result is a list of `ObsId`, in the order given."""
    ids = gs.parse_obs_ids_only([str(OTHER_OBS_ID), str(OBS_ID)])
    assert ids == [OTHER_OBS_ID, OBS_ID]
    assert all(isinstance(i, gs.ObsId) for i in ids)


def test_parse_obs_ids_only_reads_a_file(tmp_path: pathlib.Path) -> None:
    """A file has IDs that spaces and lines separate."""
    path = write_ids(tmp_path, f"{OBS_ID} {OTHER_OBS_ID}\n{OBS_ID}\n")
    assert gs.parse_obs_ids_only([path]) == [OBS_ID, OTHER_OBS_ID, OBS_ID]


def test_parse_obs_ids_only_refuses_a_job_id() -> None:
    """A Job ID among Obs IDs is an error, so that fewer jobs are not submitted."""
    with pytest.raises(gs.ParseErrorJobIdsGiven) as excinfo:
        gs.parse_obs_ids_only([str(OBS_ID), str(JOB_ID)])
    error = excinfo.value
    assert isinstance(error, gs.ParseError)
    assert error.kind == "JobIdsGiven"
    assert error.job_ids == [JOB_ID]
    assert str(JOB_ID) in str(error)


def test_parse_obs_ids_only_needs_an_obs_id() -> None:
    """No ID at all is an error."""
    with pytest.raises(gs.ParseErrorNoObsIds) as excinfo:
        gs.parse_obs_ids_only([])
    assert excinfo.value.kind == "NoObsIds"


def test_parse_job_ids_only_returns_job_ids() -> None:
    """The result is a list of `int`."""
    assert gs.parse_job_ids_only([str(JOB_ID), str(OTHER_JOB_ID)]) == [JOB_ID, OTHER_JOB_ID]


def test_parse_job_ids_only_refuses_an_obs_id() -> None:
    """An Obs ID among Job IDs is an error, and the message says how to find the Job IDs."""
    with pytest.raises(gs.ParseErrorObsIdsGiven, match=gs.OBS_ID_HINT) as excinfo:
        gs.parse_job_ids_only([str(JOB_ID), str(OBS_ID)])
    error = excinfo.value
    assert error.kind == "ObsIdsGiven"
    assert error.obs_ids == [OBS_ID]
    assert all(isinstance(o, gs.ObsId) for o in error.obs_ids)


def test_parse_job_ids_only_needs_a_job_id() -> None:
    """No ID at all is an error."""
    with pytest.raises(gs.ParseErrorNoJobIds) as excinfo:
        gs.parse_job_ids_only([])
    assert excinfo.value.kind == "NoJobIds"


def test_parse_many_reads_both_kinds_of_id(tmp_path: pathlib.Path) -> None:
    """A number with 10 digits is an Obs ID, another number is a Job ID, and a file is read."""
    path = write_ids(tmp_path, f"{OTHER_JOB_ID}\n{OTHER_OBS_ID}\n")
    job_ids, obs_ids = gs.parse_many_job_ids_or_obs_ids([str(JOB_ID), str(OBS_ID), path])
    assert job_ids == [JOB_ID, OTHER_JOB_ID]
    assert obs_ids == [OBS_ID, OTHER_OBS_ID]
    assert all(isinstance(o, gs.ObsId) for o in obs_ids)


def test_parse_many_refuses_zero() -> None:
    """0 is neither kind of ID."""
    with pytest.raises(gs.ParseErrorZeroJobId) as excinfo:
        gs.parse_many_job_ids_or_obs_ids(["0"])
    assert excinfo.value.kind == "ZeroJobId"


def test_parse_many_reports_the_word_that_is_not_a_number(tmp_path: pathlib.Path) -> None:
    """The error has the file and the word."""
    path = write_ids(tmp_path, f"{JOB_ID} oops {OBS_ID}\n")
    with pytest.raises(gs.ParseErrorInsideFile) as excinfo:
        gs.parse_many_job_ids_or_obs_ids([path])
    error = excinfo.value
    assert error.kind == "InsideFile"
    assert error.file == path
    assert error.text == "oops"


def test_parse_many_raises_os_error_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """A file that is not there is not a `ParseError`: it is Python's own error, with the file name."""
    missing = tmp_path / "missing.txt"
    with pytest.raises(FileNotFoundError) as excinfo:
        gs.parse_many_job_ids_or_obs_ids([str(missing)])
    assert excinfo.value.filename == str(missing)


def test_parse_job_ids_and_obs_ids_from_file(tmp_path: pathlib.Path) -> None:
    """The two lists of a file, each in order."""
    path = tmp_path / "ids.txt"
    path.write_text(f"{JOB_ID} {OBS_ID}\n{OTHER_JOB_ID}\n")
    job_ids, obs_ids = gs.parse_job_ids_and_obs_ids_from_file(path)
    assert job_ids == [JOB_ID, OTHER_JOB_ID]
    assert obs_ids == [OBS_ID]


def test_parse_job_ids_and_obs_ids_from_file_raises_os_error_for_a_missing_file(tmp_path: pathlib.Path) -> None:
    """The file is a `Path` here, and its absence is `FileNotFoundError`."""
    with pytest.raises(FileNotFoundError):
        gs.parse_job_ids_and_obs_ids_from_file(tmp_path / "missing.txt")


@pytest.mark.parametrize("text", ["2026-09-01T00:00:00Z", "2026-09-01"])
def test_parse_utc_time_reads_rfc_3339_and_a_date(text: str) -> None:
    """The time is aware and in UTC. A date is midnight UTC."""
    time = gs.parse_utc_time(text)
    assert time == datetime.datetime(2026, 9, 1, tzinfo=datetime.timezone.utc)
    assert time.utcoffset() == datetime.timedelta(0)


def test_parse_utc_time_reads_an_offset() -> None:
    """The time is the instant, and not the clock of the offset."""
    time = gs.parse_utc_time("2026-09-01T10:00:00+10:00")
    assert time == datetime.datetime(2026, 9, 1, tzinfo=datetime.timezone.utc)


@pytest.mark.parametrize("text", ["2026-09-01T12:00:00", "tomorrow", ""])
def test_parse_utc_time_refuses_a_time_that_it_would_have_to_guess(text: str) -> None:
    """A time with no offset is not guessed."""
    with pytest.raises(gs.ParseErrorInvalidTime, match="RFC 3339") as excinfo:
        gs.parse_utc_time(text)
    assert excinfo.value.kind == "InvalidTime"
