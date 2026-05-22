FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM alpine:3.20

RUN apk add --no-cache ca-certificates

COPY --from=builder /build/target/release/ecs-sd /ecs-sd

LABEL org.opencontainers.image.source="https://github.com/wasilak/ecs-sd"
LABEL org.opencontainers.image.description="Prometheus HTTP Service Discovery for AWS ECS"
LABEL org.opencontainers.image.licenses="MIT"

ENTRYPOINT ["/ecs-sd"]
