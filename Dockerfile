# syntax=docker/dockerfile:1.4
# Multi-stage build with cargo-chef for optimized caching

# ============================================================
# Stage 1: Chef base image with build tools
# ============================================================
FROM lukemathwalker/cargo-chef:latest-rust-1.91-bookworm AS chef
WORKDIR /app

# Install build dependencies including mold linker for faster linking
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    mold \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Configure cargo to use mold linker
RUN mkdir -p /root/.cargo && echo '[target.x86_64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]' > /root/.cargo/config.toml

# Install sccache for compiler caching
ENV SCCACHE_DIR=/sccache
RUN cargo install sccache --version ^0.8 --locked
ENV RUSTC_WRAPPER=sccache

# ============================================================
# Stage 2: Planner - analyze dependencies
# ============================================================
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ============================================================
# Stage 3: Builder - build dependencies then application
# ============================================================
FROM chef AS builder

# Copy the recipe (dependency specification)
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this layer is cached when dependencies don't change
# Using BuildKit cache mounts for sccache, cargo registry, and git
ARG SQLX_OFFLINE=true
ENV SQLX_OFFLINE=${SQLX_OFFLINE}
ENV CARGO_INCREMENTAL=0

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/sccache,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

# Copy source code
COPY . .

# Build the application
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/sccache,sharing=locked \
    cargo build --release --locked && \
    strip target/release/omikuji

# ============================================================
# Stage 4: Runtime - minimal production image
# ============================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    tzdata \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r -g 1000 omikuji && \
    useradd -r -u 1000 -g omikuji omikuji

# Copy binary from builder
COPY --from=builder /app/target/release/omikuji /usr/local/bin/omikuji

# Copy migrations from source
COPY migrations /migrations

# Create config directory and set permissions
RUN mkdir -p /config && chown omikuji:omikuji /config && \
    chown -R omikuji:omikuji /migrations

# Switch to non-root user
USER omikuji

# Set working directory
WORKDIR /config

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/metrics || exit 1

# Expose metrics port
EXPOSE 8080

# Default command
ENTRYPOINT ["/usr/local/bin/omikuji"]
CMD ["-c", "/config/config.yaml"]
