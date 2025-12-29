# syntax=docker/dockerfile:1.4

# Stage 1: Base image with Rust, cargo-chef, and dependencies
FROM ubuntu:22.04 AS base

RUN apt-get update && apt-get install -y \
    curl \
    unzip \
    build-essential \
    pkg-config \
    libssl-dev \
    cmake \
    git \
    openssh-client \
    llvm-dev \
    libclang-dev \
    binutils \
    m4 \
    file \
    google-perftools \
    libgoogle-perftools-dev && \
    rm -rf /var/lib/apt/lists/*

# Install protoc (Protobuf compiler) v21.12+
RUN curl -LO https://github.com/protocolbuffers/protobuf/releases/download/v21.12/protoc-21.12-linux-x86_64.zip && \
    unzip protoc-21.12-linux-x86_64.zip -d /usr/local && \
    rm protoc-21.12-linux-x86_64.zip

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

# Copy rust toolchain configuration
COPY rust-toolchain.toml .
RUN cargo install --locked cargo-chef && rustup show

# Stage 2: Planner - analyze project and generate recipe.json
FROM base AS planner
WORKDIR /app

# Copy workspace Cargo files
COPY Cargo.toml Cargo.lock ./
COPY src/lib.rs ./src/
COPY bin/server.rs ./bin/

# Copy build script and proto files (needed for tonic-build)
COPY build.rs ./
COPY proto ./proto

# Generate recipe for dependencies
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - build dependencies and the application
FROM base AS builder
WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json

# Copy the source files needed for cargo chef cook
COPY Cargo.toml Cargo.lock ./
COPY src/lib.rs ./src/
COPY bin/server.rs ./bin/
COPY build.rs ./
COPY proto ./proto

# Build dependencies using cargo-chef (cacheable layer)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

# Copy the entire project
COPY . .

# Build the server binary
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --bin server

# Harden artifacts: strip symbols
RUN set -eu; \
    if command -v strip >/dev/null 2>&1; then \
        strip --strip-all /app/target/release/server || true; \
    fi

# Stage 4: Runtime image
FROM ubuntu:22.04

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    google-perftools \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built binary
COPY --from=builder /app/target/release/server ./server

# Copy migrations
COPY --from=builder /app/migrations ./migrations

# Copy model.json for cost estimation
RUN mkdir -p /app/fixtures
COPY --from=builder /app/fixtures/model.json ./fixtures/model.json

# Create gnark_downloads directory (will be mounted at runtime)
RUN mkdir -p /app/gnark_downloads

# Performance settings (from README)
ENV RUSTFLAGS="-C target-cpu=native -C target-feature=+avx512f,+avx512ifma,+avx512vl"
ENV JEMALLOC_SYS_WITH_MALLOC_CONF="retain:true,background_thread:true,metadata_thp:always,dirty_decay_ms:-1,muzzy_decay_ms:-1,abort_conf:true"

# Verification (from README)
ENV VK_VERIFICATION=true

# Default environment variables (can be overridden via .env)
# Server configuration
ENV GRPC_ADDR=0.0.0.0:50052
ENV DATABASE_URL=sqlite:///app/data/pico_proving_service.db
ENV MAX_GRPC_MSG_SIZE=1073741824
ENV AUTH_METHOD=none

# Proving configuration
ENV PROVER_COUNT=32
ENV SPLIT_THRESHOLD=1048576
ENV CHUNK_SIZE=4194304
ENV CHUNK_BATCH_SIZE=1
ENV RUST_MIN_STACK=16777216

# Logging
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=full

# Expose gRPC port (default)
EXPOSE 50052

# Create non-root user for running the service
RUN useradd -m -u 1000 picouser && \
    mkdir -p /app/data && \
    chown -R picouser:picouser /app

# Install sqlite3 for database operations
RUN apt-get update && apt-get install -y sqlite3 && rm -rf /var/lib/apt/lists/*

# Switch to non-root user
USER picouser

# Volume for persistent data
VOLUME ["/app/data"]

CMD ["./server"]

