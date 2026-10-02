# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Constants for the giant-squid command line program."""

# The environment variable that turns off colour in the job table (the NO_COLOR convention). The variables that
# the library reads (the API key, the host, ...) are read by the library; see `AsvoClient.from_env`.
ENV_NO_COLOUR = "NO_COLOR"

# Bytes in a KiB, for printing sizes.
BYTES_PER_KIB = 1024

# The exit codes: 1 for a failure and 2 for a usage error, as in the Rust command.
EXIT_OK = 0
EXIT_FAILED = 1
EXIT_USAGE = 2
EXIT_INTERRUPTED = 130

# What to do when an obsid is given to a command that takes job IDs only (`wait` and `cancel`). The Rust command says
# the same.
OBS_ID_HINT = "To find the job IDs of an obsid, use 'giant-squid list <obsid>'."

# A valid obsid to build a request body from, to get the default of each option from the module (the module's
# `*_params` functions return the MWA ASVO defaults). It is never sent.
PLACEHOLDER_OBS_ID = 1_000_000_000
PLACEHOLDER_SOURCE_JOB_ID = 1
PLACEHOLDER_VOLTAGE_OFFSET = 0
PLACEHOLDER_VOLTAGE_DURATION = 1

# The log line format: the time, the level and the message, as the Rust command prints them.
LOG_FORMAT = "%(asctime)s [%(levelname)s] %(message)s"
LOG_TIME_FORMAT = "%H:%M:%S"
LOG_WARNING_NAME = "WARN"

# How often the progress bars redraw at most, in seconds.
PROGRESS_REDRAW_INTERVAL_S = 0.1

# The width of a progress bar, at most, and the least width of the rest of its line, in characters.
PROGRESS_BAR_WIDTH = 30
PROGRESS_MIN_TEXT_WIDTH = 40
DEFAULT_TERMINAL_COLUMNS = 80

# The time format of the "Completed" column of the job table.
COMPLETED_FORMAT = "%Y-%m-%d %H:%M"

# The date-only form that `--date-from` and `--date-to` accept (midnight UTC).
DATE_ONLY_FORMAT = "%Y-%m-%d"
