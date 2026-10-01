#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Check the generated stub (mwa_giant_squid.pyi) against the built module, with mypy's stubtest.
#
#   uv sync && tools/run_stubtest.sh
#
# The known differences are in tools/stubtest_allowlist.txt. The mypy version is pinned, because a new version of
# stubtest can add checks and so change the findings; change it here and in the allowlist together.
set -euo pipefail

MYPY_VERSION="2.3.1"

cd "$(dirname "$0")/.."
exec uv run --no-sync --with "mypy==${MYPY_VERSION}" python -m mypy.stubtest \
    --allowlist tools/stubtest_allowlist.txt mwa_giant_squid
