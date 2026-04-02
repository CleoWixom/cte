# ── Builder ────────────────────────────────────────────────────────────────────
FROM rust:1.86-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app

# 1. Copy manifests + lock for dependency caching layer
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml   crates/core/Cargo.toml
COPY crates/wasm/Cargo.toml   crates/wasm/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml

# 2. Stub sources so `cargo build` can pre-fetch and compile deps
RUN mkdir -p crates/core/src crates/wasm/src crates/server/src/routes && \
    printf 'pub fn main(){}' > crates/server/src/main.rs && \
    printf ''                > crates/core/src/lib.rs     && \
    printf ''                > crates/wasm/src/lib.rs

RUN cargo build --release -p trieval-server ; true
RUN rm -rf crates/*/src

# 3. Real source
COPY crates/ crates/
COPY migrations/ migrations/

RUN cargo build --release -p trieval-server

# Copy binary regardless of whether it was named 'server' or 'trieval-server'
RUN cp target/release/server /app/trieval-server 2>/dev/null || \
    cp target/release/trieval-server /app/trieval-server

# ── Runtime ────────────────────────────────────────────────────────────────────
FROM alpine:3.21

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/trieval-server /server
COPY --from=builder /app/migrations     /migrations

ENV RUST_LOG=info
EXPOSE 8080
ENTRYPOINT ["/server"]
