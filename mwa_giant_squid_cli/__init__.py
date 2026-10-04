# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The ``giant-squid`` command, run by the Rust program inside the ``mwa_giant_squid`` module.

This is the Rust ``giant-squid`` command and not a copy of it: the module runs the same code as the Rust program, so
the commands, the options, the help, the messages, the colours and the exit codes are the same. This package only
starts it. Run ``giant-squid --help``, or see ``docs/PYTHON.md``.
"""

import signal
import sys
from collections.abc import Sequence

import mwa_giant_squid


def main(argv: Sequence[str] | None = None) -> int:
    """Run the ``giant-squid`` command and return its exit code.

    This is the entry point of the ``giant-squid`` program. It puts the default action of Ctrl-C (SIGINT) back for
    the whole process, so that Ctrl-C ends the process at once, as it ends the Rust program, which has no Ctrl-C
    handler of its own. Python's handler would raise ``KeyboardInterrupt`` only when Python code next runs, and that
    does not happen while the Rust code runs. A program that calls this function gets the same change.

    Args:
        argv: The arguments after the name of the program. ``None`` is ``sys.argv[1:]``.

    Returns:
        The exit code: 0 for success (also for ``--help`` and ``--version``), 2 for a bad argument, 1 for any other
        error.
    """
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    arguments = sys.argv[1:] if argv is None else argv
    # `_run_cli` is the module's private entry point for this program.
    # `_PROGRAM_NAME` is the Rust program's name, for the usage line and the messages of the help.
    return mwa_giant_squid._run_cli([mwa_giant_squid._PROGRAM_NAME, *arguments])
