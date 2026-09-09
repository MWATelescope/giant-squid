#!/usr/bin/env bash
set -e

echo Cleaning
cargo clean

echo Upadting
cargo update --verbose

echo Cargo check...
cargo check --all-targets

echo Cargo clippy...
cargo clippy --fix --all-targets -- -D warnings

echo Cargo fmt...
cargo fmt 

echo Done
