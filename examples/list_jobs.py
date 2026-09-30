# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# List your MWA ASVO jobs with the giant-squid library only (no CLI code).
#
# The library reads no environment variables, so this program (the
# caller) reads them and builds an `AsvoClientConfig`:
#
# ```text
# MWA_ASVO_API_KEY=<your key> cargo run --example list_jobs [DAYS]
# ```
#
# Set `MWA_ASVO_HOST` to use a server other than the production MWA ASVO,
# for example `https://test-asvo.mwatelescope.org`. If `HOME` is set, the
# session is cached in the token file that giant-squid and mwa-cli share.
# `DAYS` (optional) lists only the jobs from the past `DAYS` days.
#
#
# ```text
# python list_jobs.py
# ```
import os

import mwa_giant_squid

if __name__ == "__main__":
    host = os.environ["MWA_ASVO_HOST"]
    api_key = os.environ["MWA_ASVO_API_KEY"]
    c = mwa_giant_squid.AsvoClient(host, api_key)

    jobs = c.get_jobs()

    for j in jobs:
        print(j)
