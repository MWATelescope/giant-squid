#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Run at the MSRV (`rust-version` in Cargo.toml) what CI runs at the MSRV:
#
#   - check.sh (the lint job: format, clippy and docs), and
#   - `cargo test --locked` (the "msrv" job of run-tests.yaml).
#
# Run it before a change that could need a newer Rust than the MSRV (a new
# dependency, a new standard library function, a new language feature).
#
# It changes no file and no dependency: Cargo.lock is used as it is
# (`--locked`), and nothing is cleaned or updated. It builds in `target/msrv`,
# so the build of your normal Rust is not thrown away. To use another
# directory, set CARGO_TARGET_DIR. It needs rustup, which installs the MSRV
# toolchain if it is not there.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v rustup > /dev/null; then
    echo "Error: rustup is needed to install the MSRV toolchain." >&2
    exit 1
fi

MIN_RUST="$(grep -m1 'rust-version' Cargo.toml | sed 's|.*"\(.*\)"|\1|')"
echo "MSRV: ${MIN_RUST}"
rustup toolchain install "${MIN_RUST}" --profile minimal --component rustfmt --component clippy

# Every cargo command below, and in check.sh, uses the MSRV toolchain.
export RUSTUP_TOOLCHAIN="${MIN_RUST}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/msrv}"

echo "Checking format, clippy and docs..."
./check.sh

echo "Running the tests..."
cargo test --locked
