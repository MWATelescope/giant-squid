#!/bin/bash

set -eux

# Copy the release readme to the project root so it can neatly be put in the
# release tarballs.
cp .github/workflows/releases-readme.md README.md

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # I don't know why, but I need to reinstall Rust. Probably something to do with
    # GitHub overriding env variables.
    curl https://sh.rustup.rs -sSf | sh -s -- -y

    # Build a release for each x86_64 microarchitecture level. v4 can't be
    # compiled on GitHub for some reason.
    for level in "x86-64" "x86-64-v2" "x86-64-v3"; do
        export RUSTFLAGS="-C target-cpu=${level}"

        # Build the executable
        cargo build --release

        # Create new release asset tarballs
        mv target/release/giant-squid .
        tar -acvf giant-squid-$(git describe --tags)-Linux-${level}.tar.gz \
            LICENSE README.md giant-squid
    done
elif [[ "$OSTYPE" == "darwin"* ]]; then
    cargo build --release

    mv target/release/giant-squid .
    tar -acvf giant-squid-$(git describe --tags)-MacOSX.tar.gz \
        LICENSE README.md giant-squid
fi
