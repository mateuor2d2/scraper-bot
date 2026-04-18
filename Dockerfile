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
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Copy backend binary
COPY --from=rust-builder /app/target/release/scraper-bot /app/scraper-bot

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
