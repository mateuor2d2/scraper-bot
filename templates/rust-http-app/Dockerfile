# =============================================================================
# Multi-stage Dockerfile for Rust HTTP Applications
# =============================================================================
# Usage:
#   docker build --build-arg APP_NAME=my-app -t my-app .
#   docker run -p 3000:3000 my-app
#
# Requirements:
#   - Application must expose a /health endpoint
#   - Binary name must match APP_NAME in Cargo.toml
# =============================================================================

# ---------------------------------------------------------------------------
# Stage 1: Build
# ---------------------------------------------------------------------------
FROM rust:1.85-slim-bookworm AS builder

ARG APP_NAME=app

# Install build dependencies (linker, openssl if needed)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Cache dependencies: copy manifests first and build a dummy project
COPY Cargo.toml Cargo.lock ./

RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source and build the real binary
COPY src ./src
COPY . .

# Touch main.rs so cargo detects changes after dummy build
RUN touch src/main.rs && \
    cargo build --release && \
    cp target/release/${APP_NAME} /usr/local/bin/app-binary

# ---------------------------------------------------------------------------
# Stage 2: Runtime
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

ARG APP_NAME=app

# Install minimal runtime deps (ca-certificates for HTTPS, curl for healthcheck)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user and group
RUN groupadd --gid 1000 appuser && \
    useradd --uid 1000 --gid appuser --shell /bin/bash --create-home appuser

# Application port (configurable at runtime)
ENV PORT=3000
ENV RUST_LOG=info

WORKDIR /home/appuser

# Copy binary from builder
COPY --from=builder /usr/local/bin/app-binary ./app-binary

# Copy any static assets if they exist (optional, won't fail if absent)
COPY --chown=appuser:appuser . .

# Ensure ownership
RUN chown -R appuser:appuser /home/appuser

# Switch to non-root user
USER appuser

EXPOSE ${PORT}

# Health check: expects /health endpoint on the configured PORT
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

ENTRYPOINT ["./app-binary"]
