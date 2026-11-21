# OpenWard container image: one static-asset-free binary on Debian slim.
#
#   docker compose up -d                 # start on http://localhost:3000
#   docker compose run --rm openward openward-seed --db /data/openward.db   # optional demo data

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p openward-server -p openward-seed

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin openward \
    && mkdir -p /data/backups \
    && chown -R openward:openward /data
COPY --from=build /src/target/release/openward-server /src/target/release/openward-seed /usr/local/bin/

USER openward
WORKDIR /data
ENV OPENWARD_DB=/data/openward.db \
    OPENWARD_BACKUP_DIR=/data/backups \
    OPENWARD_BIND=0.0.0.0:3000 \
    RUST_LOG=openward_server=info,tower_http=info
VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:3000/health || exit 1
CMD ["openward-server"]
