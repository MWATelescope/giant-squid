# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The job table that the ``list`` and ``wait`` commands print."""

import logging
import os
import sys
from datetime import timezone

from mwa_giant_squid import AsvoJob, AsvoJobState, AsvoJobType, AsvoJobVec

from .constants import BYTES_PER_KIB, COMPLETED_FORMAT, ENV_NO_COLOUR

log = logging.getLogger(__name__)

# The column titles.
TITLES = ("Job ID", "Obsid", "Job Type", "Job State", "File Size", "Delivery", "Completed")

# The message when there are no jobs.
NO_JOBS_MESSAGE = "You have no jobs."

# The warning when a job has a type that this version does not know.
UNKNOWN_JOB_TYPE_WARNING = (
    "giant-squid needs to be updated: one of more of your jobs contains a job_type that is unknown to this "
    "version of giant-squid. Please update to the latest version."
)

# ANSI escape codes for the colours, and for bold and reset.
RESET = "\x1b[0m"
BOLD = "\x1b[1m"
BLUE = "\x1b[34m"
YELLOW = "\x1b[33m"
MAGENTA = "\x1b[35m"
RED = "\x1b[31m"
GREEN = "\x1b[32m"
WHITE = "\x1b[37m"
BRIGHT_WHITE = "\x1b[97m"

# The colour of each job type, as in the Rust command.
JOB_TYPE_COLOURS = {
    AsvoJobType.Conversion: BLUE,
    AsvoJobType.DownloadVisibilities: BLUE,
    AsvoJobType.DownloadBeamformer: BLUE,
    AsvoJobType.DownloadMetadata: YELLOW,
    AsvoJobType.DownloadVoltage: MAGENTA,
    AsvoJobType.CancelJob: RED,
    AsvoJobType.Imaging: BLUE,
    AsvoJobType.Unknown: RED,
}

# The colour of each job state. A job with an error is red; the states in progress are magenta.
JOB_STATE_COLOURS = {
    AsvoJobState.Queued: BRIGHT_WHITE,
    AsvoJobState.Ready: GREEN,
    AsvoJobState.Error: RED,
    AsvoJobState.Expired: WHITE,
    AsvoJobState.Cancelled: RED,
}


def format_size(size: int) -> str:
    """Format a number of bytes with binary units, for example ``1.5 GiB``.

    Args:
        size: The number of bytes.

    Returns:
        The text.
    """
    if size < BYTES_PER_KIB:
        return f"{size} B"
    value = float(size)
    for unit in ("KiB", "MiB", "GiB", "TiB", "PiB"):
        value /= BYTES_PER_KIB
        if value < BYTES_PER_KIB:
            break
    return f"{value:.1f} {unit}"


def use_colour(no_colour: bool) -> bool:
    """Decide whether to colour the table.

    Args:
        no_colour: The ``--no-colour`` option.

    Returns:
        ``True`` if standard output is a terminal, ``--no-colour`` is not given and ``NO_COLOR`` is not set.
    """
    return not no_colour and sys.stdout.isatty() and ENV_NO_COLOUR not in os.environ


def state_text(job: AsvoJob) -> str:
    """Make the text of a job's state. A job with an error shows its message, as in the Rust command.

    Args:
        job: The job.

    Returns:
        The state, for example ``Ready`` or ``Error: the message``.
    """
    state = str(job.job_state)
    return f"{state}{job.error_text or ''}" if job.job_state == AsvoJobState.Error else state


def job_cells(job: AsvoJob) -> list[str]:
    """Make the text of a job's row.

    Args:
        job: The job.

    Returns:
        The text of each column.
    """
    product = job.product
    files = product.files if product is not None else []
    completed = job.completed.astimezone(timezone.utc).strftime(COMPLETED_FORMAT) if job.completed else ""
    return [
        str(job.job_id),
        str(job.obs_id),
        str(job.job_type),
        state_text(job),
        format_size(sum(f.size for f in files)) if product is not None else "",
        str(files[0].type) if files else "",
        completed,
    ]


def print_jobs_table(jobs: AsvoJobVec, no_colour: bool) -> None:
    """Print the jobs to standard output as a table, or a short message if there are none.

    Args:
        jobs: The jobs.
        no_colour: Do not colour the job type and job state.
    """
    if len(jobs) == 0:
        print(NO_JOBS_MESSAGE)
        return
    rows = [job_cells(job) for job in jobs]
    widths = [max(len(TITLES[i]), *(len(row[i]) for row in rows)) for i in range(len(TITLES))]
    colour = use_colour(no_colour)
    rule = "+" + "+".join("-" * (w + 2) for w in widths) + "+"

    def line(cells: list[str], styles: dict[int, str]) -> str:
        parts = []
        for i, (cell, width) in enumerate(zip(cells, widths, strict=True)):
            padded = cell.ljust(width)
            style = styles.get(i, "")
            parts.append(f" {style}{padded}{RESET} " if style else f" {padded} ")
        return "|" + "|".join(parts) + "|"

    print(rule)
    print(line(list(TITLES), dict.fromkeys(range(len(TITLES)), BOLD) if colour else {}))
    print(rule)
    for job, row in zip(jobs, rows, strict=True):
        styles = {}
        if colour:
            styles = {2: JOB_TYPE_COLOURS[job.job_type], 3: JOB_STATE_COLOURS.get(job.job_state, MAGENTA)}
        print(line(row, styles))
    print(rule)
    if any(job.job_type == AsvoJobType.Unknown for job in jobs):
        log.warning(UNKNOWN_JOB_TYPE_WARNING)
