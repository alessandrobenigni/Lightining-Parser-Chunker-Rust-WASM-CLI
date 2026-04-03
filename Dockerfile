# Reproducible build environment for parser-chunker
# Produces a deterministic release binary given the same source tree.
#
# Usage:
#   docker build -t parser-chunker-build .
#   docker run --rm -v $(pwd)/output:/output parser-chunker-build \
#       cp /build/target/release/parser-chunker /output/
#
# For fully reproducible builds, pin the image digest:
#   FROM rust:1.85-slim@sha256:<digest>

FROM rust:1.85-slim AS builder

# Install minimal build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependency builds: copy manifests first
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./

# Create a dummy main.rs to build dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/generate_sbom.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

# Copy actual source
COPY src/ src/
COPY benches/ benches/
COPY clippy.toml rustfmt.toml ./

# Build for real — touch source files so cargo knows to rebuild
RUN touch src/main.rs src/lib.rs && \
    cargo build --release

# Verification stage
RUN cargo test --release

# The binary is at /build/target/release/parser-chunker
# Extract it in a minimal image if desired:
FROM debian:bookworm-slim AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/parser-chunker /usr/local/bin/parser-chunker
ENTRYPOINT ["parser-chunker"]
