FROM rust:bookworm AS builder

WORKDIR /build

# Dependency caching: build only deps first using a stub main.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release
RUN rm -f target/release/ecs-sd target/release/deps/ecs_sd*

# Build the real binary; deps layer is reused as long as manifests are unchanged.
COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /build/target/release/ecs-sd /ecs-sd

LABEL org.opencontainers.image.source="https://github.com/wasilak/ecs-sd"
LABEL org.opencontainers.image.description="Prometheus HTTP Service Discovery for AWS ECS"
LABEL org.opencontainers.image.licenses="MIT"

ENTRYPOINT ["/ecs-sd"]
