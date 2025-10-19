##
# This file can be used to cross-compile, for example, from an amd64
# machine to arm64 (Raspberry Pi) or to build the app without
# installing Rust and build dependencies locally.
#
# To cross-compile the Debian package, you need a builder supporting
# the target platform. For example, to build for arm64, run the
# following command once to create the builder:
#
#     docker buildx create --name cross --bootstrap --platform linux/arm64
#
# Build the Debian package and copy it to the current directory:
#
#     docker build . --builder cross --platform linux/arm64 --target=dpkg --output type=local,dest=.
#
# Build just the binary (note that to execute it afterwards,
# libsdl2-2.0.0 dependency package need to be installed):
#
#     docker build . --builder cross --platform linux/arm64 --target=bin --output type=local,dest=.
#
# To build for the current architecture (not cross-compile), you can
# use the default builder (skip the `docker buildx` command above) and
# omit the `--builder` and `--platform` options.
#
# To build for Debian bullseye, change the build stage base image
# (`FROM ...`) to rust:bullseye.
#
# To cross-compile for 32bit Raspberry Pi, use the `--platform
# linux/arm/v7` option in the commands above. Note that currently it's
# not possible to cross-compile for arm/v6 (so e.g. RPi Zero) since
# Rust does not provide a Docker image for that architecture (see
# https://github.com/rust-lang/docker-rust/issues/54). The only option
# for RPi Zero seems to be a native compilation, which is VERY slow.
##

FROM rust:trixie AS build

RUN DEBIAN_FRONTEND=noninteractive apt update && \
    apt install -y libsdl2-dev libssl-dev lintian && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
# Cache dependencies build
RUN cargo init --name syno-photo-frame --vcs none .
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch
RUN cargo build --release
# Build the binary and Debian package
COPY ./ ./
WORKDIR /workspace/dpkg
RUN make

# The following two stages can be used as --target options for `docker
# build`, making it possible to extract the artifacts from Docker
# images to the local file system.
FROM scratch AS dpkg
COPY --from=build /workspace/dpkg/syno-photo-frame_* ./

FROM scratch AS bin
COPY --from=build /workspace/target/release/syno-photo-frame ./
