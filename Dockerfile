# Multi-stage build for scraper-bot + admin panel
# Stage 1: Build Nuxt admin panel
FROM node:22-slim AS admin-builder
WORKDIR /app/admin
COPY admin/package*.json ./
RUN npm install
COPY admin/ ./
RUN npm run generate

# Stage 2: Build Rust backend
FROM rust:1.88-slim-bookworm AS rust-builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
WORKDIR /app

# Minimal runtime deps (no Chromium!)
RUN apt-get update && apt-get install -y \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Download Obscura headless browser (replaces Chromium, ~25MB vs ~400MB)
RUN curl -LO https://github.com/h4ckf0r0day/obscura/releases/latest/download/obscura-x86_64-linux.tar.gz \
    && tar xzf obscura-x86_64-linux.tar.gz \
    && mv obscura /usr/local/bin/obscura \
    && chmod +x /usr/local/bin/obscura \
    && rm obscura-x86_64-linux.tar.gz \
    && echo "Obscura installed: $(obscura --help 2>&1 | head -1)"

# Obscura as CDP backend (auto-launched by the bot)
ENV OBSCURA_PATH=/usr/local/bin/obscura
ENV OBSCURA_PORT=9222
ENV OBSCURA_STEALTH=1

# Copy backend binary
COPY --from=rust-builder /app/target/release/scraper-bot /app/scraper-bot

# Copy config
COPY config.toml /app/config.toml

# Copy admin static files
COPY --from=admin-builder /app/admin/.output/public /app/admin

# Copy migrations
COPY --from=rust-builder /app/migrations /app/migrations

# Create data directory for SQLite
RUN mkdir -p /app/data

EXPOSE 8080
ENV RUST_LOG=info
ENV ADMIN_STATIC_DIR=/app/admin
CMD ["/app/scraper-bot"]
