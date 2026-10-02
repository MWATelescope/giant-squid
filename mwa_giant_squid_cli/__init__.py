# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The ``giant-squid`` command line program, written in Python on the ``mwa_giant_squid`` module.

It has the same commands, options, environment variables and output as the Rust ``giant-squid`` command. Run
``giant-squid --help``, or see ``docs/PYTHON.md``.
"""

import argparse
import logging
import sys
from collections.abc import Sequence

import mwa_giant_squid

from .args import build_parser, normalise_bool_flags
from .commands import (
    EXPECTED_ERRORS,
    UsageError,
    cmd_cancel,
    cmd_download,
    cmd_list,
    cmd_submit,
    cmd_submit_image_from_job,
    cmd_wait,
)
from .constants import EXIT_FAILED, EXIT_INTERRUPTED, EXIT_OK, EXIT_USAGE, LOG_FORMAT, LOG_TIME_FORMAT, LOG_WARNING_NAME
from .progress import DisplayHandler, ProgressDisplay

__all__ = ["main"]

# The attribute that marks a log handler as this program's, so that a second call to `main` replaces it.
HANDLER_MARK = "_giant_squid_cli"


def init_logging(verbosity: int, display: ProgressDisplay) -> logging.Handler:
    """Send log records to standard error, as the Rust command does.

    The records of the ``mwa_giant_squid`` module go through Python's ``logging``, so they appear too.

    Args:
        verbosity: The number of ``-v`` options: none is INFO, one or more is DEBUG. (The module sends nothing
            below DEBUG.)
        display: The progress bars, which log lines are written above.

    Returns:
        The handler that was added to the root logger, so that ``main`` can take it away again.
    """
    logging.addLevelName(logging.WARNING, LOG_WARNING_NAME)
    root = logging.getLogger()
    for handler in list(root.handlers):
        if getattr(handler, HANDLER_MARK, False):
            root.removeHandler(handler)
    handler = DisplayHandler(display)
    handler.setFormatter(logging.Formatter(LOG_FORMAT, LOG_TIME_FORMAT))
    setattr(handler, HANDLER_MARK, True)
    root.addHandler(handler)
    root.setLevel(logging.INFO if verbosity == 0 else logging.DEBUG)
    # Make the module see the new configuration.
    mwa_giant_squid.reset_logging()
    return handler


def run(args: argparse.Namespace, display: ProgressDisplay) -> None:
    """Run the command that was parsed.

    Args:
        args: The parsed arguments.
        display: The progress bars, for ``download``.
    """
    match args.command:
        case "list":
            cmd_list(args)
        case "download":
            cmd_download(args, display)
        case "submit-image-from-job":
            cmd_submit_image_from_job(args)
        case "wait":
            cmd_wait(args)
        case "cancel":
            cmd_cancel(args)
        case _:
            cmd_submit(args)


def main(argv: Sequence[str] | None = None) -> int:
    """Run the program.

    Args:
        argv: The arguments, without the program name. ``None`` uses ``sys.argv``.

    Returns:
        The exit code: 0 if it worked, 1 if it failed, 2 for a usage error such as an option that is out of range.
        (A command line that cannot be parsed exits with 2 by ``SystemExit``.)
    """
    arguments = normalise_bool_flags(list(sys.argv[1:] if argv is None else argv))
    args = build_parser().parse_args(arguments)
    display = ProgressDisplay(sys.stderr)
    root = logging.getLogger()
    level_before = root.level
    handler = init_logging(args.verbosity, display)
    try:
        run(args, display)
    except KeyboardInterrupt:
        display.close()
        print("Interrupted.", file=sys.stderr)
        return EXIT_INTERRUPTED
    except UsageError as e:
        display.close()
        print(f"error: {e}\nFor more information, try '--help'.", file=sys.stderr)
        return EXIT_USAGE
    except EXPECTED_ERRORS as e:
        display.close()
        print(f"Error: {e}", file=sys.stderr)
        return EXIT_FAILED
    finally:
        # Leave the logging as it was found. The handler writes to this run's standard error, which can be closed
        # later (a test's is); a program that calls ``main`` keeps its own logging.
        root.removeHandler(handler)
        root.setLevel(level_before)
        mwa_giant_squid.reset_logging()
    return EXIT_OK
