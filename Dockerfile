FROM rust:1.94.0-bookworm AS builder

WORKDIR /usr/src/oorouter

RUN apt-get update \
    && apt-get install --yes --no-install-recommends libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY src-tauri/Cargo.toml ./src-tauri/Cargo.toml
COPY src-tauri/src/lib.rs ./src-tauri/src/lib.rs

RUN cargo build --locked --release --package proxy-core --bin proxy-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install \
        --yes \
        --no-install-recommends \
        ca-certificates \
        libssl3 \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 oorouter \
    && useradd \
        --uid 10001 \
        --gid oorouter \
        --home-dir /home/oorouter \
        --create-home \
        --shell /usr/sbin/nologin \
        oorouter \
    && install \
        --directory \
        --mode 0755 \
        /config/codex \
    && install \
        --directory \
        --mode 0700 \
        --owner oorouter \
        --group oorouter \
        /data

COPY --from=builder /usr/src/oorouter/target/release/proxy-server /usr/local/bin/proxy-server
COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/oorouter-entrypoint

ENV HOME=/home/oorouter \
    XDG_DATA_HOME=/data \
    PORT=11434

EXPOSE 11434

USER root:root

ENTRYPOINT ["/usr/local/bin/oorouter-entrypoint"]
CMD []
