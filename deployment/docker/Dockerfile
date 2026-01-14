# Multi-stage Docker build for optimal performance and small image size
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig

# Set working directory
WORKDIR /app

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create src directory with dummy main for dependency compilation
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release
RUN rm src/main.rs target/release/atomiq*

# Copy all source code
COPY . .

# Build all binaries
RUN cargo build --release

# Runtime stage - minimal Alpine Linux
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    openssl \
    libgcc \
    curl

# Create app user for security
RUN addgroup -g 1001 -S appgroup && \
    adduser -S appuser -u 1001 -G appgroup

# Create data directory
RUN mkdir -p /data && \
    chown -R appuser:appgroup /data

# Copy binaries from builder
COPY --from=builder /app/target/release/atomiq-api /usr/local/bin/
COPY --from=builder /app/target/release/atomiq-unified /usr/local/bin/

# Copy configuration files if they exist
COPY --from=builder /app/*.toml /app/ 2>/dev/null || true

# Switch to app user
USER appuser

# Set environment variables
ENV DATABASE_PATH=/data/blockchain_data
ENV RUST_LOG=info,atomiq=debug
ENV CONSENSUS_MODE=DirectCommit
ENV MAX_CONCURRENT_REQUESTS=50000

# Expose API port
EXPOSE 8080

# Volume for persistent data
VOLUME ["/data"]

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

# Default command - run API server
CMD ["atomiq-api", "--db-path", "/data/blockchain_data", "--port", "8080", "--host", "0.0.0.0"]