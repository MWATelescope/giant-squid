# syntax=docker/dockerfile:1

# ---------- builder ----------
FROM dhi.io/rust:1-debian13-dev AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        clang \
        git \
        jq \
        lcov \
        unzip \
        zip \
        automake \
        libtool \
        ca-certificates \
    && apt-get -y autoremove \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

WORKDIR /app
COPY . .

RUN cargo install --path . --locked

# ---------- runtime ----------
FROM dhi.io/rust:1-debian13 AS runtime

# Runtime DHI images have no package manager, so pull the CA bundle
# from the builder rather than apt-installing it here.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

COPY --from=builder /root/.cargo/bin/giant-squid /usr/local/bin/giant-squid

ENTRYPOINT [ "/usr/local/bin/giant-squid" ]