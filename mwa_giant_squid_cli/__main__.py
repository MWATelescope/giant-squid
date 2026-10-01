# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Run the giant-squid command with ``python -m mwa_giant_squid_cli``."""

import sys

from mwa_giant_squid_cli import main

if __name__ == "__main__":
    sys.exit(main())
