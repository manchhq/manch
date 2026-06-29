# syntax=docker/dockerfile:1

# ── Build stage ───────────────────────────────────────
FROM rust:1.88-bookworm AS builder
WORKDIR /build
# manch-server's build.rs runs connectrpc-build, which spawns `protoc` to compile
# proto/manch/v1/manch.proto — so protoc must be installed.
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
# Copy the whole workspace: build.rs reads ../../proto and the build needs the
# workspace Cargo.toml + Cargo.lock; --locked enforces the pinned versions.
COPY . .
RUN cargo build --locked --release -p manch-server

# ── Runtime stage ─────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/manch-server /usr/local/bin/manch-server
# Bind to all interfaces inside the container (the app defaults to 127.0.0.1).
ENV MANCH_ADDR=0.0.0.0:8787
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/manch-server"]
