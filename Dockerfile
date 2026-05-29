FROM rust:bookworm AS chef

WORKDIR /build
RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /build/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --locked --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/ecs-sd /ecs-sd

LABEL org.opencontainers.image.source="https://github.com/wasilak/ecs-sd"
LABEL org.opencontainers.image.description="Prometheus HTTP Service Discovery for AWS ECS"
LABEL org.opencontainers.image.licenses="GPL-3.0-only"

ENTRYPOINT ["/ecs-sd"]
