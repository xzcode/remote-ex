# Self-host signaling only. No media capture or privileged access.
# Build from repository root: docker build -f deploy/Dockerfile -t remote-ex-rendezvous:local .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml ./
COPY crates ./crates
COPY servers ./servers
RUN cargo build --release -p remote-ex-rendezvous
FROM debian:bookworm-slim
RUN groupadd --gid 10001 remote-ex && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin remote-ex
COPY --from=build /src/target/release/remote-ex-rendezvous /usr/local/bin/remote-ex-rendezvous
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/remote-ex-rendezvous"]
