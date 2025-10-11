# Multi-stage Docker build for Snowboot
# Stage 1: Builder
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    cmake \
    make \
    gcc \
    g++

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src
COPY benches ./benches
COPY tests ./tests

# Build the real application
RUN cargo build --release --bin snowboot

# Stage 2: Runtime
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc

# Create non-root user
RUN addgroup -g 1000 snowboot && \
    adduser -D -u 1000 -G snowboot snowboot

# Create directories
RUN mkdir -p /var/lib/snowboot /var/run/snowboot && \
    chown -R snowboot:snowboot /var/lib/snowboot /var/run/snowboot

# Copy binary from builder
COPY --from=builder /build/target/release/snowboot /usr/local/bin/snowboot

# Copy config example
COPY config.example.toml /etc/snowboot/config.example.toml

# Switch to non-root user
USER snowboot

# Expose metrics and health ports (if enabled)
EXPOSE 9090 8080

# Set up entrypoint
ENTRYPOINT ["/usr/local/bin/snowboot"]

# Default arguments (can be overridden)
CMD ["--help"]
