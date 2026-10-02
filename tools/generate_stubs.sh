#!/usr/bin/env bash
#
# Regenerates the Python type stubs, mwa_giant_squid.pyi, from the
# pyo3-stub-gen annotations in src/python. Run it from the repository root
# after a change to the Python API, then review and commit the result:
#
#     tools/generate_stubs.sh
#     git diff mwa_giant_squid.pyi
#
# pyo3-stub-gen writes `typing.Optional[X]` and its own layout, so ruff
# then changes the stub to the project style (`X | None`, line length 120).
# It assumes cargo and uv are on the PATH, and that `uv run` can run ruff in
# the project (`uv sync` makes the environment). It must run from the
# repository root, because its paths are relative. It has been tested on
# Linux only.
#
# stub_gen is a normal program that embeds Python, so it is linked against
# a libpython. pyo3 builds for the Python in PYO3_PYTHON when that variable
# is set, and for the first python3 on the PATH when it is not. This script
# does not set it. build.rs puts that Python's library directory in the
# rpath of stub_gen, so no LD_LIBRARY_PATH is needed, even for a uv-managed
# Python (which keeps libpython in its own directory). After a build with
# `cargo build --no-default-features --features python-stubgen`, you can
# also run target/debug/stub_gen yourself.

# Fail the script on any error
set -euo pipefail

readonly STUB_FILE="mwa_giant_squid.pyi"

cargo build --no-default-features --features python-stubgen
target/debug/stub_gen
uv run ruff check --fix --quiet "${STUB_FILE}"
uv run ruff format --quiet "${STUB_FILE}"
