#!/usr/bin/env bash
#
# Regenerates the Python type stubs, mwa_giant_squid.pyi, from the
# pyo3-stub-gen annotations in src/python. Run it after a change to the
# Python API, then review and commit the result:
#
#     tools/generate_stubs.sh
#     git diff mwa_giant_squid.pyi
#
# pyo3-stub-gen writes `typing.Optional[X]` and its own layout, so ruff
# then changes the stub to the project style (`X | None`, line length 120).
# It assumes cargo and uv are on the PATH. It can run from any directory.

# Fail the script on any error
set -euo pipefail

cd "$(dirname "$0")/.."

readonly STUB_FILE="mwa_giant_squid.pyi"

cargo run --no-default-features --features python-stubgen --bin stub_gen
uv run ruff check --fix --quiet "${STUB_FILE}"
uv run ruff format --quiet "${STUB_FILE}"
