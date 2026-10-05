# Base image source: https://raw.githubusercontent.com/MusicalNinjaDad/rust/refs/heads/main/Dockerfile
FROM ghcr.io/musicalninjadad/rust-devcontainer:latest@sha256:9bd652e79ab2ca0f53a5f7e5faf566ea961127fcc7d74f3529a82929499ccf87

# for nativetls
RUN sudo dnf install openssl-devel pkgconf-pkg-config

# Cross compilation
RUN cargo binstall -y cargo-xwin \
    && cargo xwin cache windows-msvc-sysroot \
    && cargo xwin cache xwin \
    && rustup target add x86_64-pc-windows-msvc \
    && rustup install --target x86_64-pc-windows-msvc nightly \
    && rustup +nightly component add miri

RUN cargo binstall -y ripgrep

# Slint UI
RUN cargo binstall -y \
        slint-lsp \
        slint-viewer \
    && sudo dnf install \
        fontconfig-devel \
        libinput \
        wayland-devel \
        xorg-x11-server-Xwayland

# Windows exe metadata
RUN cargo binstall -y svg_to_ico \
    && sudo dnf install \
        clang \
        llvm
