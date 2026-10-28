# Build and test environment for Linux, same base as the release build (glibc 2.35).
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl pkg-config git \
        libfontconfig1-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
        libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libegl1-mesa-dev \
        libgl1-mesa-dri dbus-x11 file \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --component clippy --default-toolchain stable

# Build output stays in the container volume, not on the Windows checkout.
ENV CARGO_TARGET_DIR=/target
WORKDIR /src
