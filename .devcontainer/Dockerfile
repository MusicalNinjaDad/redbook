# Base image source: https://raw.githubusercontent.com/MusicalNinjaDad/rust/refs/heads/main/Dockerfile
FROM ghcr.io/musicalninjadad/rust-devcontainer:latest@sha256:793f0e83422b268826b84772f3010891065542c517af08f0a2ee54ffbab5416b

RUN sudo dnf install openssl-devel pkgconf-pkg-config
RUN cargo binstall -y cargo-xwin jaq uv
RUN cargo-xwin xwin cache windows-msvc-sysroot \
    && cargo-xwin xwin cache xwin
