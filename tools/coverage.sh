#!/usr/bin/env bash

# Fail the script on any error
set -euo pipefail

#
# A local test coverage report for giant-squid, to run before pushing.
#
# It measures the Rust code, which is all the code the package has: the
# library, the CLI, and the Python bindings in src/python/, which the
# Python tests run through the compiled extension module. It runs:
#
#   1. the Rust tests (unit and CLI), with the `python` feature, so the
#      binding code is compiled too;
#   2. the Python tests (pytest), against an instrumented build of the
#      extension module.
#
# and prints two summaries: after the Rust tests alone, and with the
# Python tests added. The line coverage of src/python/ in the second is
# the "Python test coverage". The live tests (tests/live.rs) are ignored,
# as in CI, and nothing contacts a real MWA ASVO server.
#
# Outputs, in coverage/ (git-ignored):
#   coverage/html/index.html   the combined report, file by file
#   coverage/coverage.lcov     the combined report, for an editor plugin
#                              (for example VS Code "Coverage Gutters")
#
# The generated src/asvo/apiv2/openapi.rs is left out of the report: it is
# typify's output (builders and conversions for every schema type, most of
# which the client never runs), so it would only hide the numbers of the
# code that is written by hand. Its correctness is checked by the
# openapi-drift-check CI job and by the tests of the types that are used.
#
# The "Generate Coverage report" CI workflow uses the same tool
# (cargo-llvm-cov) and settings.
#
# It assumes:
# 1. You run it from anywhere inside the repository.
# 2. You have `cargo-llvm-cov` (`cargo install cargo-llvm-cov`) and the
#    LLVM tools for your Rust toolchain (`rustup component add
#    llvm-tools`). With a toolchain not installed by rustup, set
#    LLVM_COV and LLVM_PROFDATA to its llvm-cov and llvm-profdata.
# 3. You have `uv` (see pyproject.toml).
#
# The Python tests need the extension module built with coverage
# instrumentation, so this script rebuilds it into .venv. Run `uv sync`
# afterwards to go back to a normal build.
#

REPO_DIR=$(git rev-parse --show-toplevel)
cd "${REPO_DIR}"

# The directory for the reports (matches the CI workflow).
OUT_DIR="coverage"
# The files that the report leaves out (a regular expression of paths).
COVERAGE_IGNORE_REGEX='src/asvo/apiv2/openapi\.rs'
# The maturin that builds the extension (the range in pyproject.toml).
MATURIN_REQUIREMENT="maturin>=1.9.4,<2.0"

if ! cargo llvm-cov --version > /dev/null 2>&1; then
    echo "Error: cargo-llvm-cov not found. Install it with: cargo install cargo-llvm-cov" >&2
    exit 1
fi
if ! command -v uv > /dev/null 2>&1; then
    echo "Error: uv not found. See https://docs.astral.sh/uv/" >&2
    exit 1
fi

# Do not ask to install llvm-tools; fail with the tool's message instead.
export CARGO_LLVM_COV_SETUP="${CARGO_LLVM_COV_SETUP:-no}"

# Set the environment for instrumented builds, and build everything into
# the target dir that `cargo llvm-cov report` reads (as PyO3's own coverage
# run does), including the extension module that maturin builds.
source <(cargo llvm-cov show-env --sh)
export CARGO_TARGET_DIR="${CARGO_LLVM_COV_TARGET_DIR}"

# Remove the results of any earlier run.
cargo llvm-cov clean --workspace
rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

# A test failure does not stop the script: the reports are still written,
# for the tests that ran, and the script exits with the failure at the end.
tests_status=0

echo "=== 1/2: Rust tests"
cargo test --features python || tests_status=$?

echo
echo "=== Coverage from the Rust tests"
cargo llvm-cov report --summary-only --ignore-filename-regex "${COVERAGE_IGNORE_REGEX}"

echo
echo "=== 2/2: Python tests, on an instrumented extension module"
# The dev tools only; the project itself is built next, instrumented.
uv sync --no-install-project
VIRTUAL_ENV="${REPO_DIR}/.venv" uvx --from "${MATURIN_REQUIREMENT}" maturin develop --uv --profile dev
uv run --no-sync pytest -q -p no:cacheprovider || tests_status=$?

echo
echo "=== Coverage from the Rust and Python tests"
cargo llvm-cov report --summary-only --ignore-filename-regex "${COVERAGE_IGNORE_REGEX}"
cargo llvm-cov report --html --output-dir "${OUT_DIR}" --ignore-filename-regex "${COVERAGE_IGNORE_REGEX}"
cargo llvm-cov report --lcov --output-path "${OUT_DIR}/coverage.lcov" --ignore-filename-regex "${COVERAGE_IGNORE_REGEX}"

echo
echo "HTML report: ${OUT_DIR}/html/index.html"
echo "LCOV report: ${OUT_DIR}/coverage.lcov"
echo "The extension module in .venv is instrumented; run 'uv sync' to rebuild it normally."

if (( tests_status != 0 )); then
    echo "Error: some tests failed (see above); the report covers the tests that ran." >&2
    exit "${tests_status}"
fi
