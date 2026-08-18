FROM rust:1.95-bookworm AS build-stage1

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libxml2-dev \
    libxmlsec1-dev \
    libxmlsec1-openssl \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY src/rust/odo-client/ odo-client/
COPY src/rust/odo-entity/ odo-entity/
COPY src/rust/odo-service/ odo-service/

WORKDIR /build/odo-service

# Download dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo fetch

FROM build-stage1 AS build-stage2

WORKDIR /build

COPY src/rust/odo-auth/ odo-auth/

WORKDIR /build/odo-auth

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/odo-auth/target \
    cargo build --release --features saml && \
    cp target/release/odo-auth /usr/local/bin/odo-auth

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libxmlsec1 \
    libxmlsec1-openssl \
    libxml2 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1010 appuser

COPY --from=build-stage2 /usr/local/bin/odo-auth /usr/local/bin/odo-auth

USER appuser
WORKDIR /app

ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/odo-auth"]
