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

# Install Chromium + dependencies for browser automation
RUN apt-get update && apt-get install -y \
    ca-certificates curl \
    chromium chromium-driver \
    fonts-liberation libappindicator3-1 libasound2 libatk-bridge2.0-0 \
    libatk1.0-0 libc6 libcairo2 libcups2 libdbus-1-3 libexpat1 \
    libfontconfig1 libgbm1 libgcc1 libglib2.0-0 libgtk-3-0 libnspr4 \
    libnss3 libpango-1.0-0 libpangocairo-1.0-0 libstdc++6 libx11-6 \
    libx11-xcb1 libxcb1 libxcomposite1 libxcursor1 libxdamage1 \
    libxext6 libxfixes3 libxi6 libxrandr2 libxrender1 libxss1 \
    libxtst6 lsb-release wget xdg-utils \
    && rm -rf /var/lib/apt/lists/*

# Tell chromiumoxide where to find Chrome
ENV CHROME_PATH=/usr/bin/chromium
ENV CHROMIUM_FLAGS="--headless --no-sandbox --disable-dev-shm-usage --disable-gpu"

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
