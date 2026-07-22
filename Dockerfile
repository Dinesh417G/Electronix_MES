# Multi-stage build for the MES service binaries (mes-edge / mes-cloud).
# One image ships both; docker-compose selects which to run via `command:`.

FROM rust:1-bookworm AS builder
WORKDIR /build

# Copy the whole workspace. A dependency-only pre-build cache layer is skipped
# here for M0 simplicity; add it if CI build time becomes a concern.
COPY . .
RUN cargo build --release --bin mes-edge --bin mes-cloud

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /build/target/release/mes-edge /usr/local/bin/mes-edge
COPY --from=builder /build/target/release/mes-cloud /usr/local/bin/mes-cloud

# Default to the edge service; overridden per compose service.
CMD ["mes-edge"]
