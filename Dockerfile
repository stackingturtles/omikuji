# Multi-stage build for minimal final image
# Builder stage
FROM rust:1.91-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /build

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Copy SQLX offline query data
COPY .sqlx ./.sqlx

# Create dummy main.rs for dependency caching
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN SQLX_OFFLINE=true cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Copy migrations
COPY migrations ./migrations

# Build the application
RUN SQLX_OFFLINE=true cargo build --release && \
    strip target/release/omikuji

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r -g 1000 omikuji && \
    useradd -r -u 1000 -g omikuji omikuji

# Copy binary from builder
COPY --from=builder /build/target/release/omikuji /usr/local/bin/omikuji

# Copy migrations from builder
COPY --from=builder /build/migrations /migrations

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