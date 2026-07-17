# syntax=docker/dockerfile:1

# --- Stage 1: build ---------------------------------------------------------
FROM rust:1.92-bookworm AS build

WORKDIR /src

# libsqlite3-sys is vendored ("bundled" feature), so sqlite3.c compiles with the
# toolchain already in the rust image -- no system libsqlite3 needed. There is
# no build.rs and no embed_migrations! macro (schema is inline SQL in db.rs), so
# the whole crate just needs its own source tree.
COPY . .

# Build only the daemon binary in release (LTO + codegen-units=1 come from the
# crate's [profile.release]). The target dir is a cache mount, so we copy the
# finished binary out of it within the same layer to capture it in the image.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --bin deciduous && \
    cp /src/target/release/deciduous /usr/local/bin/deciduous

# --- Stage 2: runtime -------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# curl: used by the container HEALTHCHECK below.
# sqlite3: used by deploy/backup.sh (online .backup) against the data volume.
# ca-certificates: harmless; future-proofs any outbound TLS the tool may add.
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl sqlite3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Unprivileged user that owns the data directory.
RUN useradd --system --create-home --uid 10001 deciduous && \
    mkdir -p /data/graphs && chown -R deciduous:deciduous /data

COPY --from=build /usr/local/bin/deciduous /usr/local/bin/deciduous

USER deciduous
WORKDIR /home/deciduous

# Graphs live at /data/graphs/<id>/deciduous.db -- persisted via a named volume.
VOLUME ["/data"]

ENV DECIDUOUS_DATA_DIR=/data \
    DECIDUOUS_PORT=4141 \
    DECIDUOUS_BIND=0.0.0.0

EXPOSE 4141

# Authenticated liveness: 200 => process up, token valid, graphs dir readable.
# $DECIDUOUS_API_TOKEN expands at runtime inside the container's shell, so the
# token is never written into image metadata.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS -H "Authorization: Bearer ${DECIDUOUS_API_TOKEN}" \
      "http://127.0.0.1:${DECIDUOUS_PORT}/api/v1/graphs" >/dev/null || exit 1

# Bind 0.0.0.0 *inside the container only*. docker-compose.yml deliberately does
# NOT publish this port, so from the internet's point of view the daemon is
# loopback-only -- reachable solely by Caddy over the private compose network.
# Token is read from DECIDUOUS_API_TOKEN in the environment (not --token).
ENTRYPOINT ["/bin/sh", "-c", "exec deciduous serve --api --bind \"$DECIDUOUS_BIND\" --port \"$DECIDUOUS_PORT\" --data-dir \"$DECIDUOUS_DATA_DIR\""]
