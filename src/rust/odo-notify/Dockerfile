FROM rust:1.95-bookworm AS build-stage1

WORKDIR /build

COPY src/rust/odo-client/ odo-client/
COPY src/rust/odo-entity/ odo-entity/
COPY src/rust/odo-service/ odo-service/

WORKDIR /build/odo-service

RUN --mount=type=cache,target=/usr/local/cargo/registry cargo fetch

FROM build-stage1 AS build-stage2

WORKDIR /build

COPY src/rust/odo-notify/ odo-notify/

WORKDIR /build/odo-notify

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/odo-notify/target \
    cargo build --release && \
    cp target/release/odo-notify /usr/local/bin/odo-notify

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1010 appuser

COPY --from=build-stage2 /usr/local/bin/odo-notify /usr/local/bin/odo-notify

USER appuser
WORKDIR /app

ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/odo-notify"]
