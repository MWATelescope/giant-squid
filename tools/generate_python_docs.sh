#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Generate the Python API documentation (HTML) with pdoc, from mwa_giant_squid.pyi.
#
#   uv sync && tools/generate_python_docs.sh [output directory]
#
# The default output directory is target/python-docs. The docs are made from an importable copy of the stub, which
# tools/generate_python_docs.py writes; it reads the enum values from the built module, so build it first (uv sync).
# The .github/workflows/python-docs.yaml workflow publishes the result to GitHub Pages.
#
# The pdoc version is pinned, because a new version can change the pages; change it here only.
set -euo pipefail

PDOC_VERSION="16.0.0"
DEFAULT_OUTPUT_DIR="target/python-docs"
STUB_FILE="mwa_giant_squid.pyi"
# The copy must have the name of the module, so that the pages do too.
MODULE_FILE="mwa_giant_squid.py"

cd "$(dirname "$0")/.."
output_dir="${1:-${DEFAULT_OUTPUT_DIR}}"

work_dir="$(mktemp -d)"
trap 'rm -rf "${work_dir}"' EXIT

uv run --no-sync python tools/generate_python_docs.py "${STUB_FILE}" "${work_dir}/${MODULE_FILE}"

# pdoc runs in an environment of its own, without the built module, so that it can only import the copy.
rm -rf "${output_dir}"
uvx --from "pdoc==${PDOC_VERSION}" pdoc --docformat google --no-show-source \
    --output-directory "${output_dir}" "${work_dir}/${MODULE_FILE}"
