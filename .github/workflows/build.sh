#!/bin/bash

set -eux

# Copy the release readme to the project root so it can neatly be put in the
# release tarballs.
cp .github/workflows/releases-readme.md README.md

 # determine which target cpus for rustc to build for from machine type
export ARCH="$(uname -m)"

if [[ "$OSTYPE" == "linux-gnu"* ]]; then    
    case $ARCH in      
      x86_64)        
        export TARGETS="x86-64 x86-64-v2 x86-64-v3";;
      aarch64)
        export TARGETS="aarch64";;
    esac

    # Build a release for each target
    for target in $TARGETS; do
        export RUSTFLAGS="-C target-cpu=${target}"

        # Build the executable
        cargo build --release

        # Create new release asset tarballs
        mv target/release/giant-squid .
        tar -acvf giant-squid-$(git describe --tags)-Linux-${target}.tar.gz \
            LICENSE README.md giant-squid
    done

elif [[ "$OSTYPE" == "darwin"* ]]; then
    cargo build --release

    mv target/release/giant-squid .
    
    # HOSTYPE should by x86_64 or arm64
    tar -acvf giant-squid-$(git describe --tags)-MacOS-${HOSTTYPE}.tar.gz \
    LICENSE README.md giant-squid    
fi
