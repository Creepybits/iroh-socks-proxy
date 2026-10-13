# Sovereign daemon in a container.
#
# By default it publishes the files in /site (copied from the repo's `site/`
# folder) at the daemon's .anon address. The secret key, and so the address,
# lives in /data: mount a volume there or the address changes on every restart.
#
#   docker build -t sovereign-daemon .
#   docker run -d -v anon-data:/data sovereign-daemon
#
# Arguments after the image name replace `--serve /site`, e.g. `--forward ...`.

ARG RUST_IMAGE=rust:1.97-slim-trixie
ARG RUNTIME_IMAGE=debian:trixie-slim

FROM ${RUST_IMAGE} AS build
WORKDIR /src
COPY daemon/ ./
RUN cargo build --release --locked

FROM ${RUNTIME_IMAGE}
RUN useradd --system --uid 10001 --user-group --home-dir /data --no-create-home sovereign \
    && mkdir -p /data \
    && chown sovereign:sovereign /data
# The daemon talks HTTPS to Iroh's relay and lookup servers, so it needs the
# certificate authorities list; the build image already has an up-to-date one.
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /src/target/release/sovereign-daemon /usr/local/bin/sovereign-daemon
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
COPY site/ /site/
VOLUME /data
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["--serve", "/site"]
