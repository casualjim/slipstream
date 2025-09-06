# Use cargo-chef to maximize dependency caching across builds
# Build arguments (global): PACKAGE selects the crate to build, BIN_NAME optionally
# overrides the produced binary name. Declare them before any FROM so they are
# available to all stages (and can be used in FROM if needed).
ARG PACKAGE=slipstream-server
ARG BIN_NAME
FROM lukemathwalker/cargo-chef:latest-rust-1.89.0-alpine AS chef
WORKDIR /app

# System deps for native builds (cmake, compilers, protobuf, etc.)
FROM chef AS chef-deps
RUN apk add --no-cache \
  build-base \
  cmake \
  pkgconfig \
  clang20-dev \
  protobuf-dev \
  openssl-dev

# 1) Planner: compute the recipe. We copy the full workspace to satisfy cargo metadata
# for crates that rely on implicit targets (src/lib.rs or src/main.rs).
FROM chef-deps AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2) Cacher: build only dependencies
FROM chef-deps AS cacher
ARG TARGETARCH
# System deps for building some crates (e.g., prost-build requires protoc)
COPY --from=planner /app/recipe.json recipe.json
RUN case "$TARGETARCH" in \
        amd64) TARGET_TRIPLE="x86_64-unknown-linux-musl" ;; \
        arm64) TARGET_TRIPLE="aarch64-unknown-linux-musl" ;; \
        *) echo "Unsupported architecture: $TARGETARCH"; exit 1 ;; \
    esac &&\
    cargo chef cook --release --target ${TARGET_TRIPLE} --recipe-path recipe.json --workspace

# 3) Builder: build the actual binary
FROM chef-deps AS builder
# Copy the full workspace
COPY . .
# Reuse the dependency cache
COPY --from=cacher /app/target /app/target
ARG PACKAGE
ARG TARGETARCH
ARG BIN_NAME=${BIN_NAME:-$PACKAGE}
# Build release binary for the selected package. You can override at build-time:
# docker build --build-arg PACKAGE=my-crate --build-arg BIN_NAME=my-binary .
# Build and copy the binary.
RUN case "$TARGETARCH" in \
        amd64) TARGET_TRIPLE="x86_64-unknown-linux-musl" ;; \
        arm64) TARGET_TRIPLE="aarch64-unknown-linux-musl" ;; \
        *) echo "Unsupported architecture: $TARGETARCH"; exit 1 ;; \
    esac &&\
    cargo build --release --package ${PACKAGE} --target ${TARGET_TRIPLE} &&\
    cp target/release/${TARGET_TRIPLE}/${BIN_NAME} /bin/app

# 4) Runtime: small image with just the binary
FROM alpine AS runtime
RUN apk add --no-cache ca-certificates
ENV RUST_LOG=info
# Non-root user
RUN adduser -D -u 10001 appuser
COPY --from=builder /bin/app /usr/local/bin/app
USER appuser
# Expose server port (adjust if your server uses a different default)
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/app"]

