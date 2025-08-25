# Use cargo-chef to maximize dependency caching across builds
FROM lukemathwalker/cargo-chef:latest-rust-1.89 AS chef
WORKDIR /app

# System deps for native builds (cmake, compilers, protobuf, etc.)
FROM chef AS chef-deps
RUN apt-get update && apt-get install -y --no-install-recommends \
  build-essential \
  cmake \
  pkg-config \
  clang \
  protobuf-compiler \
  libprotobuf-dev \
  && rm -rf /var/lib/apt/lists/*

# 1) Planner: compute the recipe. We copy the full workspace to satisfy cargo metadata
# for crates that rely on implicit targets (src/lib.rs or src/main.rs).
FROM chef-deps AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2) Cacher: build only dependencies
FROM chef-deps AS cacher
# System deps for building some crates (e.g., prost-build requires protoc)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --workspace

# 3) Builder: build the actual binary
FROM chef-deps AS builder
# Copy the full workspace
COPY . .
# Reuse the dependency cache
COPY --from=cacher /app/target /app/target
# Which package to build (defaults to the server)
ARG BIN=slipstream-server
# Build release binary for the selected package
RUN cargo build --release --package ${BIN}
# Keep a stable path for the runtime stage
RUN cp target/release/${BIN} /bin/app

# 4) Runtime: small image with just the binary
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
ENV RUST_LOG=info
# Non-root user
RUN useradd -m -u 10001 appuser
COPY --from=builder /bin/app /usr/local/bin/app
USER appuser
# Expose server port (adjust if your server uses a different default)
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/app"]
