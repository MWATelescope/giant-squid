# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Progress bars for downloads, drawn on standard error, with log lines that appear above them.

The bars are drawn only when standard error is a terminal, as the Rust command's are. They need no package:
they use ANSI escape codes to move the cursor.
"""

import logging
import shutil
import threading
import time
from typing import TextIO

from mwa_giant_squid import DownloadProgress

from .constants import (
    DEFAULT_TERMINAL_COLUMNS,
    PROGRESS_BAR_WIDTH,
    PROGRESS_MIN_TEXT_WIDTH,
    PROGRESS_REDRAW_INTERVAL_S,
)
from .table import format_size

# The ANSI codes that move the cursor up N lines, and clear from the cursor to the end of the screen.
CURSOR_UP = "\x1b[{}A"
CLEAR_TO_END = "\x1b[J"

# Seconds in a minute and an hour, for the elapsed and remaining time.
SECONDS_PER_MINUTE = 60
SECONDS_PER_HOUR = 3600


def format_duration(seconds: float) -> str:
    """Format a time as ``H:MM:SS``.

    Args:
        seconds: The time.

    Returns:
        The text.
    """
    whole = int(seconds)
    hours, rest = divmod(whole, SECONDS_PER_HOUR)
    minutes, secs = divmod(rest, SECONDS_PER_MINUTE)
    return f"{hours}:{minutes:02d}:{secs:02d}"


class Bar:
    """The progress of one download."""

    def __init__(self, display: "ProgressDisplay") -> None:
        """Make a bar that is not yet shown.

        Args:
            display: The display that draws the bar.
        """
        self._display = display
        self.label = ""
        self.total = 0
        self.position = 0
        self.started_at = time.monotonic()
        self.start_position = 0
        self.shown = False

    def update(self, event: DownloadProgress) -> None:
        """Change the bar for a progress event. This is the ``progress`` callback of a download.

        Args:
            event: The event.
        """
        if isinstance(event, DownloadProgress.Started):
            self.label = event.label
            self.total = event.total_bytes
            self.position = event.position
            self.start_position = event.position
            self.started_at = time.monotonic()
            self.shown = True
            self._display.redraw(force=True)
        elif isinstance(event, DownloadProgress.Advanced):
            self.position += event.bytes
            self._display.redraw()
        elif isinstance(event, DownloadProgress.Finished):
            self.shown = False
            self._display.redraw(force=True)

    def render(self, columns: int) -> str:
        """Draw the bar as one line.

        Args:
            columns: The width of the terminal.

        Returns:
            The line, no wider than ``columns - 1`` characters.
        """
        elapsed = time.monotonic() - self.started_at
        rate = (self.position - self.start_position) / elapsed if elapsed > 0 else 0.0
        remaining = (self.total - self.position) / rate if rate > 0 else 0.0
        fraction = self.position / self.total if self.total else 0.0
        stats = (
            f"{format_size(self.position)}/{format_size(self.total)} "
            f"({format_size(int(rate))}/s, {format_duration(elapsed)}, eta: {format_duration(remaining)})"
        )
        room = columns - 1 - len(stats) - len(self.label) - 5
        width = max(0, min(PROGRESS_BAR_WIDTH, room))
        filled = int(width * fraction)
        bar = f"[{'#' * filled}{'-' * (width - filled)}] " if width else ""
        return f"{self.label} {bar}{stats}"[: columns - 1]


class ProgressDisplay:
    """A set of progress bars, one for each download that is running."""

    def __init__(self, stream: TextIO) -> None:
        """Make a display.

        Args:
            stream: Where to draw: standard error. The bars show only if it is a terminal.
        """
        self.stream = stream
        self.enabled = stream.isatty()
        self._lock = threading.RLock()
        self._bars: list[Bar] = []
        self._drawn_lines = 0
        self._last_draw = 0.0

    def add_bar(self) -> Bar:
        """Add a bar for a download.

        Returns:
            The bar. It shows when its download starts.
        """
        bar = Bar(self)
        with self._lock:
            self._bars.append(bar)
        return bar

    def _columns(self) -> int:
        return shutil.get_terminal_size((DEFAULT_TERMINAL_COLUMNS, 1)).columns

    def _clear(self) -> None:
        if self._drawn_lines:
            self.stream.write(CURSOR_UP.format(self._drawn_lines) + "\r" + CLEAR_TO_END)
            self._drawn_lines = 0

    def _draw(self) -> None:
        columns = max(self._columns(), PROGRESS_MIN_TEXT_WIDTH)
        shown = [bar for bar in self._bars if bar.shown]
        for bar in shown:
            self.stream.write(bar.render(columns) + "\n")
        self._drawn_lines = len(shown)
        self.stream.flush()

    def redraw(self, force: bool = False) -> None:
        """Draw the bars again, at most every ``PROGRESS_REDRAW_INTERVAL_S`` seconds unless forced.

        Args:
            force: Draw now.
        """
        if not self.enabled:
            return
        now = time.monotonic()
        with self._lock:
            if not force and now - self._last_draw < PROGRESS_REDRAW_INTERVAL_S:
                return
            self._last_draw = now
            self._clear()
            self._draw()

    def write_line(self, text: str) -> None:
        """Write a line above the bars, as a log message.

        Args:
            text: The line, without a newline.
        """
        with self._lock:
            self._clear()
            self.stream.write(text + "\n")
            if self.enabled:
                self._draw()
            else:
                self.stream.flush()

    def close(self) -> None:
        """Remove the bars from the screen."""
        with self._lock:
            for bar in self._bars:
                bar.shown = False
            self._clear()
            self.stream.flush()


class DisplayHandler(logging.StreamHandler):
    """A log handler that writes above the progress bars, so that a message does not break a bar."""

    def __init__(self, display: ProgressDisplay) -> None:
        """Make a handler.

        Args:
            display: The display to write through.
        """
        super().__init__(display.stream)
        self._display = display

    def emit(self, record: logging.LogRecord) -> None:
        """Write a record.

        Args:
            record: The record.
        """
        try:
            self._display.write_line(self.format(record))
        except Exception:  # noqa: BLE001
            self.handleError(record)
