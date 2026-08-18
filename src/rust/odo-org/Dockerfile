FROM rust:1.95-bookworm AS build-stage1

WORKDIR /build

COPY src/rust/odo-client/ odo-client/
COPY src/rust/odo-entity/ odo-entity/
COPY src/rust/odo-service/ odo-service/

WORKDIR /build/odo-service

# Download dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo fetch

FROM build-stage1 AS build-stage2

WORKDIR /build

COPY src/rust/odo-org/ odo-org/

WORKDIR /build/odo-org

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/odo-org/target \
    cargo build --release && \
    cp target/release/odo-org /usr/local/bin/odo-org

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1010 appuser

COPY --from=build-stage2 /usr/local/bin/odo-org /usr/local/bin/odo-org

USER appuser
WORKDIR /app

ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/odo-org"]
