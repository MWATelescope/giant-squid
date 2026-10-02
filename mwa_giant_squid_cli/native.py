# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""The ``giant-squid`` command, run by the Rust program inside the ``mwa_giant_squid`` module.

This is the Rust ``giant-squid`` command and not a copy of it: the module runs the same code as the Rust program, so
the commands, the options, the help, the messages and the exit codes are the same. This file only starts it.

TEMPORARY: it is installed as ``giant-squid-native`` while the Python command in this package still exists. The next
step of the thin-client work makes it the ``giant-squid`` command and removes the rest of this package.
"""

import signal
import sys

import mwa_giant_squid

# The name of the program, for the usage line and the messages of the help.
PROGRAM_NAME = "giant-squid"


def main() -> None:
    """Run the ``giant-squid`` command with the arguments of this process, and end the process with its exit code.

    Ctrl-C ends the process at once, as it does the Rust program, which has no Ctrl-C handler of its own. Python's
    handler would raise ``KeyboardInterrupt`` only when Python code next runs, and that does not happen while the
    Rust code runs. So the default action of the signal is put back first.
    """
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    # `_run_cli` is the module's private entry point for this program.
    sys.exit(mwa_giant_squid._run_cli([PROGRAM_NAME, *sys.argv[1:]]))


if __name__ == "__main__":
    main()
