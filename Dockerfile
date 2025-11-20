# AdapterOS Production Dockerfile
# Multi-stage build for optimized production images

# =============================================================================
# BUILD STAGE: Compile Rust application
# =============================================================================
FROM rust:1.75-slim AS builder

# Install system dependencies for compilation
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./

# Copy all source code
COPY crates/ ./crates/
COPY src/ ./src/
COPY migrations/ ./migrations/
COPY scripts/ ./scripts/
COPY metal/ ./metal/

# Build the application with optimizations
RUN cargo build --release --workspace --exclude adapteros-lora-mlx-ffi

# =============================================================================
# RUNTIME STAGE: Minimal production image
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    sqlite3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN groupadd -r adapteros && useradd -r -g adapteros adapteros

# Create necessary directories with proper permissions
RUN mkdir -p /app/var /app/logs /app/cache && \
    chown -R adapteros:adapteros /app

# Copy compiled binaries from builder stage
COPY --from=builder /app/target/release/adapteros-server /app/
COPY --from=builder /app/target/release/adapteros-cli /app/
COPY --from=builder /app/target/release/aosctl /app/

# Copy runtime assets
COPY migrations/ /app/migrations/
COPY scripts/ /app/scripts/
COPY metal/ /app/metal/

# Set proper permissions on executables
RUN chmod +x /app/adapteros-server /app/adapteros-cli /app/aosctl && \
    chmod +x /app/scripts/*.sh

# Switch to non-root user
USER adapteros

# Set working directory
WORKDIR /app

# Expose ports (adjust based on configuration)
EXPOSE 8080 8443

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/healthz || exit 1

# Default command
CMD ["./adapteros-server"]

# =============================================================================
# DEVELOPMENT STAGE: Full development environment
# =============================================================================
FROM builder AS development

# Install additional development tools
RUN apt-get update && apt-get install -y \
    git \
    vim \
    htop \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Keep root user for development flexibility
USER root

# Install cargo tools for development
RUN cargo install cargo-watch cargo-audit cargo-license

# Copy development configuration
COPY .github/ ./.github/
COPY docs/ ./docs/

# Set development working directory
WORKDIR /app

# Default command for development
CMD ["cargo", "build"]

# =============================================================================
# METRICS & MONITORING IMAGE
# =============================================================================
FROM runtime AS monitoring

# Install monitoring tools
RUN apt-get update && apt-get install -y \
    prometheus-node-exporter \
    && rm -rf /var/lib/apt/lists/*

# Copy monitoring configuration
COPY docker/monitoring/ /app/monitoring/

# Expose metrics port
EXPOSE 9100

# Run both application and monitoring
CMD ["./scripts/run-with-monitoring.sh"]

# =============================================================================
# DATABASE MIGRATION IMAGE
# =============================================================================
FROM runtime AS migration

# Set migration-specific entrypoint
ENTRYPOINT ["./aosctl", "db", "migrate"]

# =============================================================================
# LABELS AND METADATA
# =============================================================================

# Standard OCI labels
LABEL org.opencontainers.image.title="AdapterOS" \
      org.opencontainers.image.description="AI Model Router and Orchestrator" \
      org.opencontainers.image.vendor="AdapterOS" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.source="https://github.com/rogu3bear/adapter-os"

# Security labels
LABEL org.opencontainers.image.security.scan="cargo-audit,cargo-license"

# Build information (set at build time)
ARG BUILD_DATE
ARG VCS_REF
LABEL org.opencontainers.image.created="$BUILD_DATE" \
      org.opencontainers.image.revision="$VCS_REF"

