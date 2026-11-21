# syntax=docker/dockerfile:1
# Deux images à partir du même commit : `--target app` et `--target worker`.
# Images de base non épinglées (mises à jour par la CI hebdomadaire) ; les binaires sont
# statiques (musl), donc indépendants de la glibc de l'image.

FROM node:lts-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:slim AS rust
# Fonctions Cargo en plus (ex. `gmflow-worker/test-utils` pour docker-compose.dev.yml).
ARG FEATURES=""
RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install && rustup target add x86_64-unknown-linux-musl
COPY . .
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --target x86_64-unknown-linux-musl \
        -p gmflow-app -p gmflow-worker ${FEATURES:+--features "$FEATURES"} \
    && mkdir /out \
    && cp target/x86_64-unknown-linux-musl/release/gmflow-app target/x86_64-unknown-linux-musl/release/gmflow-worker /out/

FROM debian:stable-slim AS app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home gmflow \
    && mkdir -p /data/fichiers && chown gmflow:gmflow /data/fichiers
COPY --from=rust /out/gmflow-app /usr/local/bin/gmflow-app
COPY --from=web /web/build /srv/web
ENV GMFLOW_WEB=/srv/web GMFLOW_FICHIERS=/data/fichiers
USER gmflow
EXPOSE 8347
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s CMD ["/usr/local/bin/gmflow-app", "sante"]
CMD ["/usr/local/bin/gmflow-app"]

FROM node:lts-slim AS worker
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code \
    && npm cache clean --force \
    && mkdir -p /data/fichiers && chown node:node /data/fichiers
COPY --from=rust /out/gmflow-worker /usr/local/bin/gmflow-worker
ENV GMFLOW_FICHIERS=/data/fichiers GMFLOW_CLAUDE_BIN=claude DISABLE_AUTOUPDATER=1
USER node
HEALTHCHECK --interval=60s --timeout=10s --start-period=60s CMD ["/usr/local/bin/gmflow-worker", "sante"]
CMD ["/usr/local/bin/gmflow-worker"]
