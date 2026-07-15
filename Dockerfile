# MegaGate CLI — multi-stage Docker build
# Usage: docker build -t megagate . --build-arg PACKAGE=megagate
#        docker run megagate mg --help

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

ARG PACKAGE=megagate
RUN cargo build --release --package "${PACKAGE}" && \
    cp target/release/mg /mg

# ---- Runtime stage ----
FROM ${RUNTIME_IMAGE}
COPY --from=build /mg /usr/local/bin/mg
ENTRYPOINT ["/usr/local/bin/mg"]
CMD ["--help"]
