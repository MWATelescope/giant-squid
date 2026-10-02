"""Tests for the helpers of the ``giant-squid`` command line program: parsing, the table and the progress bars."""

import argparse
import io
import logging
from datetime import datetime, timedelta, timezone

import pytest

import mwa_giant_squid as gs
from mwa_giant_squid_cli import parsing
from mwa_giant_squid_cli.progress import CLEAR_TO_END, DisplayHandler, ProgressDisplay, format_duration
from mwa_giant_squid_cli.table import format_size

# A progress bar's file.
LABEL = "Job ID 5 (obsid: 1065880128) [1/1]:"
TOTAL_BYTES = 2048
HALF = TOTAL_BYTES // 2


class FakeTerminal(io.StringIO):
    """A text stream that says it is a terminal."""

    def isatty(self) -> bool:
        """Say that this is a terminal.

        Returns:
            ``True``.
        """
        return True


@pytest.mark.parametrize(
    ("size", "text"),
    [
        (0, "0 B"),
        (1023, "1023 B"),
        (1024, "1.0 KiB"),
        (1536, "1.5 KiB"),
        (1610612736, "1.5 GiB"),
        (1024**4, "1.0 TiB"),
    ],
)
def test_sizes_use_binary_units(size: int, text: str) -> None:
    """A size is shown with one decimal and a binary unit."""
    assert format_size(size) == text


@pytest.mark.parametrize(("seconds", "text"), [(0, "0:00:00"), (59.9, "0:00:59"), (61, "0:01:01"), (3725, "1:02:05")])
def test_durations_are_hours_minutes_seconds(seconds: float, text: str) -> None:
    """A duration is ``H:MM:SS``."""
    assert format_duration(seconds) == text


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("2026-09-01", datetime(2026, 9, 1, tzinfo=timezone.utc)),
        ("2026-09-01T00:00:00Z", datetime(2026, 9, 1, tzinfo=timezone.utc)),
        ("2026-09-01T00:00:00z", datetime(2026, 9, 1, tzinfo=timezone.utc)),
        ("2026-09-01T08:00:00+08:00", datetime(2026, 9, 1, 8, tzinfo=timezone(timedelta(hours=8)))),
    ],
)
def test_times_are_a_date_or_rfc_3339(text: str, expected: datetime) -> None:
    """A date is midnight UTC; an RFC 3339 time keeps its offset."""
    assert parsing.parse_utc_time(text) == expected


@pytest.mark.parametrize("text", ["2026-09-01T10:00:00", "2026-9-1x", "yesterday", ""])
def test_a_time_without_an_offset_is_refused(text: str) -> None:
    """A time with no offset is refused rather than guessed, as in the Rust command."""
    with pytest.raises(argparse.ArgumentTypeError):
        parsing.parse_utc_time(text)


@pytest.mark.parametrize(
    ("text", "value"), [("true", True), ("TRUE", True), ("1", True), ("false", False), ("No", False), ("0", False)]
)
def test_booleans(text: str, value: bool) -> None:
    """A boolean option takes true or false, in any case."""
    assert parsing.parse_bool(text) is value


def test_a_bad_boolean_is_refused() -> None:
    """Other text is not a boolean."""
    with pytest.raises(argparse.ArgumentTypeError):
        parsing.parse_bool("maybe")


def test_positive_integers() -> None:
    """A positive integer is 1 or more."""
    assert parsing.positive_int("7") == 7
    for bad in ("0", "-3", "x"):
        with pytest.raises(argparse.ArgumentTypeError):
            parsing.positive_int(bad)


def test_names_ignore_case_and_punctuation() -> None:
    """Job states and types are matched on their letters only, so ``Download_Visibilities`` works."""
    parse_types = parsing.name_list_parser(parsing.JOB_TYPE_NAMES, "job type")
    parse_states = parsing.name_list_parser(parsing.JOB_STATE_NAMES, "job state")

    assert parse_types("Download_Visibilities, conversion") == [
        gs.AsvoJobType.DownloadVisibilities,
        gs.AsvoJobType.Conversion,
    ]
    assert parse_states("WAIT-CAL,ready") == [gs.AsvoJobState.WaitCal, gs.AsvoJobState.Ready]
    with pytest.raises(argparse.ArgumentTypeError):
        parse_states("bogus")


def test_every_job_state_and_type_can_be_named() -> None:
    """The names cover every member of the two enums, except the unknown type, which cannot be filtered by."""
    assert set(parsing.JOB_STATE_NAMES.values()) == {
        getattr(gs.AsvoJobState, n)
        for n in dir(gs.AsvoJobState)
        if isinstance(getattr(gs.AsvoJobState, n), gs.AsvoJobState)
    }
    assert set(parsing.JOB_TYPE_NAMES.values()) == {
        getattr(gs.AsvoJobType, n)
        for n in dir(gs.AsvoJobType)
        if isinstance(getattr(gs.AsvoJobType, n), gs.AsvoJobType) and n != "Unknown"
    }


def test_enum_values_match_by_api_value_or_name() -> None:
    """``all_fits`` and ``AllFits`` are the same output mode."""
    parse = parsing.enum_parser(gs.OutputMode, "output mode")

    assert parse("all_fits") is gs.OutputMode.AllFits
    assert parse("AllFits") is gs.OutputMode.AllFits
    with pytest.raises(argparse.ArgumentTypeError, match="choose from all_files, all_fits, fits"):
        parse("everything")


@pytest.mark.parametrize(
    ("enum_type", "values"),
    [
        (gs.Delivery, ["acacia", "dug", "scratch"]),
        (gs.DeliveryFormat, ["files", "tar"]),
        (gs.Output, ["ms", "uvfits"]),
        (gs.Centre, ["custom", "phase", "pointing"]),
        (gs.OutputMode, ["all_files", "all_fits", "fits"]),
        (gs.Polarization, ["XX", "XXYY", "YY"]),
        (gs.Weighting, ["briggs", "natural", "uniform"]),
    ],
)
def test_enum_values_lists_the_api_values_in_order(enum_type: type, values: list[str]) -> None:
    """The help of an enum option lists the API values, in alphabetical order like the Rust command."""
    assert parsing.enum_values(enum_type) == values


def test_progress_bars_are_drawn_and_removed_on_a_terminal() -> None:
    """A bar shows from its start event, advances, and is cleared when the file finishes."""
    stream = FakeTerminal()
    display = ProgressDisplay(stream)
    bar = display.add_bar()

    bar.update(gs.DownloadProgress.Started(5, LABEL, TOTAL_BYTES, 0))
    bar.update(gs.DownloadProgress.Advanced(HALF))
    display.redraw(force=True)
    shown = stream.getvalue()
    bar.update(gs.DownloadProgress.Finished())

    assert LABEL in shown
    assert "1.0 KiB/2.0 KiB" in shown
    assert stream.getvalue().endswith(CLEAR_TO_END)


def test_progress_is_not_drawn_when_not_on_a_terminal() -> None:
    """Redirected output has no bars."""
    stream = io.StringIO()
    display = ProgressDisplay(stream)
    bar = display.add_bar()

    bar.update(gs.DownloadProgress.Started(5, LABEL, TOTAL_BYTES, 0))
    bar.update(gs.DownloadProgress.Advanced(HALF))
    bar.update(gs.DownloadProgress.Finished())

    assert stream.getvalue() == ""


def test_a_log_line_is_written_above_the_bars() -> None:
    """A log record clears the bars, prints its line, and draws the bars again below it."""
    stream = FakeTerminal()
    display = ProgressDisplay(stream)
    bar = display.add_bar()
    bar.update(gs.DownloadProgress.Started(5, LABEL, TOTAL_BYTES, 0))
    before = len(stream.getvalue())
    logger = logging.getLogger("test_cli_units.progress")
    logger.propagate = False
    handler = DisplayHandler(display)
    handler.setFormatter(logging.Formatter("%(message)s"))
    logger.addHandler(handler)
    try:
        logger.warning("a message")
    finally:
        logger.removeHandler(handler)

    written = stream.getvalue()[before:]
    assert written.index(CLEAR_TO_END) < written.index("a message\n") < written.index(LABEL)
