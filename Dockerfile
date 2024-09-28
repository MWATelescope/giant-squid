FROM mwatelescope/mwalib:latest-python3.11-slim-bookworm

ENV DEBIAN_FRONTEND=noninteractive
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
    libtool \
    && \
    apt-get -y autoremove && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

ADD . /app
WORKDIR /app

RUN cargo install --path . --locked && \
    cargo clean

ENTRYPOINT [ "/opt/cargo/bin/giant-squid" ]