FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ARG DEBUG
RUN apt-get update \
    && apt-get install -y \
    build-essential \
    clang \
    curl \
    git \
    jq \
    lcov \
    libssl-dev \
    pkg-config \
    unzip \
    zip \
    automake \
    libtool

RUN test -z "$DEBUG" || ( \
    apt-get install -y vim gdb \
    )
RUN apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Get Rust
RUN mkdir -m755 /opt/rust /opt/cargo
ENV RUSTUP_HOME=/opt/rust CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y

# install latest stable rust toolchian, with llvm-tools-preview (for coverage)
RUN rustup toolchain install 1.70 --component llvm-tools-preview
# Get cargo make, llvm-cov
RUN /opt/cargo/bin/cargo install --force cargo-llvm-cov

ADD . /app
WORKDIR /app

RUN cargo clean \
    && cargo install --path . $(test -z "$DEBUG" || echo "--debug") \
    && cargo clean

ENTRYPOINT [ "/opt/cargo/bin/giant-squid" ]