# minimal urna image (issue #75): the static linux-musl binary, built from
# source in a throwaway toolchain stage, copied into scratch. no shell, no
# package manager, no network at runtime. the corpus arrives as a mounted
# volume, so the same image serves air-gapped environments.
#
# the binary is the engine-only cli: `--no-default-features` leaves out the
# terminal ui (`setup`, `tui`), which has nothing to run against in scratch,
# and `--locked` builds the dependency versions of Cargo.lock, nothing newer.
# the toolchain image is pinned by tag and digest; bump both together.
#
# build:  docker build --platform=linux/amd64 -t urna .
# run:    docker run --rm -v "$PWD/data:/data:ro" urna validate /data/corpus_next.v1.urna
#
# TARGET is a build arg: x86_64-unknown-linux-musl (default) or
# aarch64-unknown-linux-musl. on apple silicon, build the aarch64 variant
# natively (qemu user emulation crashes rustc mid-build):
#   docker build --build-arg TARGET=aarch64-unknown-linux-musl -t urna .
# debian's musl-tools provides musl-gcc for the host arch on both, which is
# why the recipe stays single-file; a multi-arch manifest is the follow-up.

FROM rust:1.98.1-bookworm@sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e AS build
ARG TARGET=x86_64-unknown-linux-musl
ENV CC_x86_64_unknown_linux_musl=musl-gcc \
    CC_aarch64_unknown_linux_musl=musl-gcc
RUN rustup target add ${TARGET} \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --locked --profile dist -p urna --no-default-features --target ${TARGET}

FROM scratch
ARG TARGET=x86_64-unknown-linux-musl
COPY --from=build /src/target/${TARGET}/dist/urna /urna
ENTRYPOINT ["/urna"]
