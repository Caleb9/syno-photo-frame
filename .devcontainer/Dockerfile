FROM debian:bookworm
ENV USER root
RUN rm /bin/sh \
    && ln -s /bin/bash /bin/sh
RUN mkdir /build \
    && cd \
    && apt update \
    && apt install -y curl build-essential \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && source $HOME/.cargo/env \
    && rustup target add aarch64-unknown-linux-gnu \
    && dpkg --add-architecture arm64 \
    && apt update \
    && apt install -y gcc-aarch64-linux-gnu libsdl2-dev:arm64 libsdl2-image-dev:arm64 libsdl2-ttf-dev:arm64
ENTRYPOINT []