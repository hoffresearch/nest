# minimal nest image (issue #75): the static linux-musl binary, built from
# source in a throwaway toolchain stage, copied into scratch. no shell, no
# package manager, no network at runtime. the corpus arrives as a mounted
# volume, so the same image serves air-gapped environments.
#
# build:  docker build --platform=linux/amd64 -f docker/Dockerfile -t nest .
# run:    docker run --rm -v "$PWD/dat:/dat:ro" nest validate /dat/corpus_next.v1.nest
#
# x86_64 only for now: debian's musl-tools cross toolchain covers
# x86_64-unknown-linux-musl; aarch64 needs a second toolchain and is left
# to the multi-arch follow-up.

FROM rust:1-bookworm AS build
RUN rustup target add x86_64-unknown-linux-musl \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --profile dist -p nest-cli --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=build /src/target/x86_64-unknown-linux-musl/dist/nest /nest
ENTRYPOINT ["/nest"]
