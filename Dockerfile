# ─── Stage 1: Rust Backend Build ──────────────────────────────────────────────
FROM rust:1.83-slim AS rust-builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

# Build release binary
RUN cargo build --release --bin nova-api

# ─── Stage 2: Frontend Build ──────────────────────────────────────────────────
FROM node:22-slim AS web-builder

RUN corepack enable pnpm

WORKDIR /app
COPY apps/web/package.json apps/web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY apps/web ./
RUN pnpm build

# ─── Stage 3: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Rust binary
COPY --from=rust-builder /app/target/release/nova-api /app/nova-api

# Copy SvelteKit build output (served by the Rust API or a reverse proxy)
COPY --from=web-builder /app/build /app/web

# Copy migrations for runtime migration
COPY migrations /app/migrations

# Create data directories
RUN mkdir -p /data/library /data/inbox /data/covers

ENV LIBRARY_DIR=/data/library
ENV INBOX_DIR=/data/inbox
ENV HOST=0.0.0.0
ENV PORT=8080

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
    CMD curl -f http://localhost:8080/api/health || exit 1

ENTRYPOINT ["/app/nova-api"]
