# Reproducible Benchmark Environment for MagiCore
# Ubuntu 22.04 + Node 20 + Rust + Package Managers
# Built: 2026-08-27

FROM ubuntu:22.04

# Prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=UTC

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev \
    time \
    jq \
    ca-certificates \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20.x (LTS)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && . $HOME/.cargo/env \
    && rustup default stable

ENV PATH="/root/.cargo/bin:${PATH}"

# Install pnpm (latest)
RUN npm install -g pnpm

# Install Bun (latest)
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:${PATH}"

# Verify installations
RUN node --version && \
    npm --version && \
    pnpm --version && \
    bun --version && \
    cargo --version && \
    rustc --version

# Create benchmark workspace
WORKDIR /benchmark

# Copy MagiCore binary (will be mounted at runtime)
# VOLUME /benchmark/mgc

# Clean state for benchmarks
RUN npm cache clean --force && \
    pnpm store prune && \
    rm -rf ~/.bun/install/cache

CMD ["/bin/bash"]
