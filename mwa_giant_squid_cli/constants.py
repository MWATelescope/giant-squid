# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Constants for the giant-squid command line program."""

# The environment variables that the program reads. The Rust command reads the same ones.
ENV_API_KEY = "MWA_ASVO_API_KEY"
ENV_HOST = "MWA_ASVO_HOST"
ENV_API_TIMEOUT = "MWA_ASVO_API_TIMEOUT"
ENV_HOME = "HOME"
ENV_BUF_SIZE = "GIANT_SQUID_BUF_SIZE"
ENV_DOWNLOAD_RETRY_SECS = "GIANT_SQUID_DOWNLOAD_RETRY_SECS"
ENV_DELIVERY = "GIANT_SQUID_DELIVERY"
ENV_DELIVERY_FORMAT = "GIANT_SQUID_DELIVERY_FORMAT"
ENV_NO_COLOUR = "NO_COLOR"

# The production MWA ASVO.
DEFAULT_HOST = "https://asvo.mwatelescope.org:443"

# The session cache that giant-squid and manta-ray-client share, relative to the home directory.
TOKEN_CACHE_PARTS = (".mwa-asvo", "tokens.json")

# Byte units, for the buffer size (given in MiB) and for printing sizes.
BYTES_PER_KIB = 1024
BYTES_PER_MIB = BYTES_PER_KIB * BYTES_PER_KIB

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

# The MWA ASVO endpoints, which a dry run reports. They are the library's `ENDPOINT_*` constants.
ENDPOINT_JOBS = "/api/v2/jobs"
ENDPOINT_CONVERSION_JOB = "/api/v2/conversion_job"
ENDPOINT_DOWNLOAD_VIS_JOB = "/api/v2/download_vis_job"
ENDPOINT_VOLTAGE_JOB = "/api/v2/voltage_job"
ENDPOINT_BEAMFORMER_JOB = "/api/v2/beamformer_job"
ENDPOINT_IMAGING_JOB = "/api/v2/imaging_job"
ENDPOINT_IMAGE_FROM_JOB = "/api/v2/image_from_job"

# The time between job list requests while waiting for jobs, and the wait before the first one, in seconds.
WAIT_POLL_INTERVAL_S = 60.0
WAIT_INITIAL_DELAY_S = 1.0

# The number of concurrent downloads, by default. 0 means one for each CPU core.
DEFAULT_CONCURRENT_DOWNLOADS = 4

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
