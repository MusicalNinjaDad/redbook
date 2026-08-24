# Base image source: https://raw.githubusercontent.com/MusicalNinjaDad/rust/refs/heads/main/Dockerfile
FROM ghcr.io/musicalninjadad/rust-devcontainer:latest@sha256:793f0e83422b268826b84772f3010891065542c517af08f0a2ee54ffbab5416b

# TODO - add to base image
RUN cargo binstall -y jaq

# for nativetls
RUN sudo dnf install openssl-devel pkgconf-pkg-config

# Cross compilation
RUN cargo binstall -y cargo-xwin \
    && cargo xwin cache windows-msvc-sysroot \
    && cargo xwin cache xwin \
    && rustup target add x86_64-pc-windows-msvc

# From fork, for mistral support
RUN cargo binstall -y uv \
    && sudo mkdir /opt/graphify \
    && sudo chown 1000:1000 /opt/graphify \
    && git clone --branch=feat/vibe-install https://github.com/xavierpestel-ai/graphify /opt/graphify \
    && uv tool install /opt/graphify
