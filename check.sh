#!/usr/bin/env bash
#
# Checks the Rust code. This script does not change any file or dependency:
# it uses Cargo.lock as it is (--locked), and it does not fix anything.
# The CI workflow .github/workflows/lint.yaml runs this script, so a pass here
# means a pass in CI. CI uses the Rust version in `rust-version` of Cargo.toml; a newer
# Rust can report more. To fix a finding, run `cargo fmt` or `cargo clippy --fix`.
#
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

# Rustdoc warnings are errors, as for the published docs.
export RUSTDOCFLAGS="-D warnings"

echo "Cargo fmt..."
cargo fmt --check

echo "Cargo clippy (default features, all targets)..."
cargo clippy --locked --all-targets -- -D warnings

echo "Cargo clippy (library only)..."
cargo clippy --locked --no-default-features --lib -- -D warnings

echo "Cargo clippy (Python module)..."
cargo clippy --locked --no-default-features --features python --lib -- -D warnings

echo "Cargo build (library only)..."
cargo build --locked --no-default-features

echo "Cargo doc..."
cargo doc --locked --no-deps

echo "Done"
