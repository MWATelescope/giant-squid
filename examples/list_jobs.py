# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""List your MWA ASVO jobs with the mwa_giant_squid module, as examples/list_jobs.rs does with the Rust library.

The module reads no environment variables when it makes an ``AsvoClientConfig``, so this program (the caller)
reads them:

```text
MWA_ASVO_API_KEY=<your key> python examples/list_jobs.py [DAYS]
```

Set ``MWA_ASVO_HOST`` to use a server other than the production MWA ASVO, for example
``https://test-asvo.mwatelescope.org``. If ``HOME`` is set, the session is cached in the token file that
giant-squid and mwa-cli share. ``DAYS`` (optional) lists only the jobs from the past ``DAYS`` days.
(``mwa_giant_squid.client_config_from_env()`` reads the same variables as the ``giant-squid`` command.)
"""

import os
import sys

import mwa_giant_squid as gs

# The position of the optional DAYS argument on the command line.
DAYS_ARGUMENT = 1


def main() -> int:
    """List the jobs, then say how many are ready for download.

    Returns:
        The exit code: 1 if the API key is not set, else 0.
    """
    api_key = os.environ.get(gs.ENV_MWA_ASVO_API_KEY)
    if api_key is None:
        print(f"{gs.ENV_MWA_ASVO_API_KEY} is not set", file=sys.stderr)
        return 1
    host = os.environ.get(gs.ENV_MWA_ASVO_HOST, gs.DEFAULT_ASVO_HOST)
    days = int(sys.argv[DAYS_ARGUMENT]) if len(sys.argv) > DAYS_ARGUMENT else None
    home = os.environ.get(gs.ENV_HOME)
    token_cache_path = gs.default_token_cache_path(home) if home is not None else None

    client = gs.AsvoClient(gs.AsvoClientConfig(host, api_key, token_cache_path=token_cache_path))
    jobs = client.get_jobs(gs.JobsFilter(days=days))

    if len(jobs) == 0:
        print("You have no jobs.")
        return 0

    print(f"{'Job ID':>10}  {'Obsid':>10}  {'Type':<24}  State")
    for job in jobs:
        job_type = str(job.job_type) if job.job_type is not None else ""
        print(f"{job.job_id:>10}  {job.obs_id!s:>10}  {job_type:<24}  {job.job_state}")

    # An example of using the module's filters: only the ready jobs.
    ready = jobs.filter(job_states=[gs.JobState.Completed])
    print(f"{len(ready)} of the jobs are ready for download.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
