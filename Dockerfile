# syntax=docker/dockerfile:1.7
#
# Much lighter than maplayercake's image (no Mapnik/Boost/HarfBuzz): this
# binary only needs a Rust toolchain. The one sibling repo it depends on as a
# Cargo path dependency (osmflat-ext = { path = "../osmflat-ext/osmflat-ext" })
# is pulled in via a named build context (see docker-build.sh), mirroring how
# maplayercake's own Dockerfile handles the same dependency. `osmflat-ext` is
# itself a two-member Cargo workspace (osmflat-ext, osmflat-extc); the whole
# workspace root is copied because cargo resolves the path dependency's
# workspace by walking up to the root `[workspace]` manifest and validates
# every listed member exists, even though only the `osmflat-ext` member is
# actually used here. `osmflat` itself is a plain git dependency, fetched
# over the network during the build like any local `cargo build` would.

FROM rust:1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /build/osmflat-ext /build/osmflat-taginfo

COPY --from=osmflat-ext Cargo.toml /build/osmflat-ext/
COPY --from=osmflat-ext osmflat-ext/ /build/osmflat-ext/osmflat-ext/
COPY --from=osmflat-ext osmflat-extc/ /build/osmflat-ext/osmflat-extc/

COPY Cargo.toml /build/osmflat-taginfo/
COPY src/ /build/osmflat-taginfo/src/

WORKDIR /build/osmflat-taginfo
RUN cargo build --release --features serve


FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/osmflat-taginfo/target/release/osmflat-taginfo /usr/local/bin/osmflat-taginfo

EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/osmflat-taginfo", "serve"]
