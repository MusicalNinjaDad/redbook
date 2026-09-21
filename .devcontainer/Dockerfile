# Base image source: https://raw.githubusercontent.com/MusicalNinjaDad/rust/refs/heads/main/Dockerfile
FROM ghcr.io/musicalninjadad/rust-devcontainer:latest@sha256:1b30b78b015756ca060a2b6e60a70e55ca4f7d6ba97212d4207aea59674850ca

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
