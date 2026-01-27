# syntax=docker/dockerfile:1.6

# Rust build stage - generates types and builds server binary
FROM rust:1.89-slim-bookworm AS builder

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
     pkg-config libssl-dev ca-certificates perl make g++ clang libclang-dev \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY shared shared
COPY assets assets

RUN mkdir -p /app/bin

# Generate TypeScript types and build server binary
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo run --release --bin generate_types \
 && cargo build --locked --release --bin server \
 && cp target/release/server /app/bin/server

# Frontend build stage
FROM node:24-alpine AS fe-builder

WORKDIR /app

RUN corepack enable

# Copy package files for dependency caching
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY frontend/package.json frontend/package.json

# Create shared directory structure for pnpm workspace
RUN mkdir -p shared

RUN --mount=type=cache,id=pnpm,target=/pnpm/store \
    pnpm install --frozen-lockfile

ARG POSTHOG_API_KEY
ARG POSTHOG_API_ENDPOINT

ENV VITE_PUBLIC_POSTHOG_KEY=$POSTHOG_API_KEY
ENV VITE_PUBLIC_POSTHOG_HOST=$POSTHOG_API_ENDPOINT

# Copy generated types from Rust builder
COPY --from=builder /app/shared ./shared

COPY frontend/ frontend/

RUN cd frontend && pnpm run build

# Runtime stage
FROM alpine:latest AS runtime

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    tini \
    libgcc \
    wget

# Create app user for security
RUN addgroup -g 1001 -S appgroup && \
    adduser -u 1001 -S appuser -G appgroup

# Copy binary from builder
COPY --from=builder /app/bin/server /usr/local/bin/server

# Create repos directory and set permissions
RUN mkdir -p /repos && \
    chown -R appuser:appgroup /repos

# Switch to non-root user
USER appuser

# Set runtime environment
ENV HOST=0.0.0.0
ENV PORT=3000
EXPOSE 3000

# Set working directory
WORKDIR /repos

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --quiet --tries=1 --spider "http://${HOST:-localhost}:${PORT:-3000}" || exit 1

# Run the application
ENTRYPOINT ["/sbin/tini", "--"]
CMD ["server"]
