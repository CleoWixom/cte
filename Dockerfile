FROM rust:1.75-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app
COPY . .

# Build only the server binary
RUN cargo build --release -p trieval-server

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM alpine:3.19

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/server /server
COPY --from=builder /app/migrations /migrations

ENV RUST_LOG=info
EXPOSE 8080

ENTRYPOINT ["/server"]
