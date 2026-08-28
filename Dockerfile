# MagiCore CLI — multi-stage Docker build
# Usage: docker build -t magicore . --build-arg PACKAGE=magicore
#        docker run magicore mgc --help

ARG RUST_IMAGE=docker.io/library/rust:1.85-slim-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12

# ---- Build stage ----
FROM ${RUST_IMAGE} AS build
WORKDIR /build

RUN apt-get update -qq && apt-get install -y -qq \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY adapters/ adapters/
COPY cli/ cli/
COPY tools/ tools/

ARG PACKAGE=magicore
RUN cargo build --release --package "${PACKAGE}" && \
    cp target/release/mgc /mgc

# ---- Runtime stage ----
FROM ${RUNTIME_IMAGE}
COPY --from=build /mgc /usr/local/bin/mgc
ENTRYPOINT ["/usr/local/bin/mgc"]
CMD ["--help"]
