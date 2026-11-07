---
name: remote-sandbox-environment
description: Load this skill when you are a remote agent running in a fresh, unprepared Linux sandbox (no root, tools not preinstalled) and need to bootstrap a working build/test environment for this repo. (The `devcontainer-environment` skill covers local agents in the prebuilt devcontainer.)
---

# Remote Sandbox Environment

Use this when you are **not** in the devcontainer: a fresh Debian/Ubuntu-based Linux sandbox without root (`sudo` fails with "no new privileges"), where `cargo-stage`/`jaq`/`ninja-xtask` are not preinstalled and system dev libraries (OpenSSL, fontconfig) are missing.

## Bootstrap

Run the bundled script once, then source the generated env file in every shell you use:

```bash
sh .vibe/skills/remote-sandbox-environment/bootstrap.sh
. /tmp/redbook-env.sh
```

The script downloads the needed `.deb`s with `apt-get` into user-writable dirs (no root), extracts them with `dpkg -x` into `/tmp/local`, and writes `/tmp/redbook-env.sh` with the required environment variables. It is idempotent.

Why each piece is needed:

- `libssl-dev` — `openssl-sys` (via reqwest/musicbrainz_rs native-tls) needs headers + lib; set `OPENSSL_NO_PKG_CONFIG=1` + `OPENSSL_INCLUDE_DIR`/`OPENSSL_LIB_DIR`. Debian keeps `opensslconf.h` in the multiarch include dir, so the script copies it next to the other OpenSSL headers.
- `libfontconfig-dev` (+ freetype/expat deps) — slint in `redbook-rip` needs fontconfig; CI installs exactly this.
- `pkgconf`/`libpkgconf3` — a `pkg-config` wrapper script is placed in `/tmp/bin` (on PATH via the env file) so build scripts can resolve `.pc` files with `PKG_CONFIG_SYSROOT=/tmp/local`.
- `RUSTFLAGS` with `-L` and `-Wl,--disable-new-dtags,-rpath,<libdir>` — **RPATH, not RUNPATH**: `libfreetype.so.6` is a *transitive* dependency (via `libfontconfig`), and RUNPATH does not apply to transitive deps, so the rpath must be a real RPATH. This makes test binaries run without `LD_LIBRARY_PATH` — important because this repo's `.cargo/config.toml` force-sets `LD_LIBRARY_PATH = ""` and the `ninja-xtask` workspace metadata unsets it for child processes.

## Rust tools

- The pinned nightly toolchain from `rust-toolchain.toml` is auto-installed by rustup on first `cargo` invocation. `rg` is usually already present.
- `jaq` (required by the `jaq` skill): `cargo install jaq`
- `cargo-stage` (the `ninja-xtask` crate): **must be built with nightly and `--locked`** — a plain `cargo install ninja-xtask` on stable fails with `unresolved import try_v2::Try`:

  ```bash
  cargo +nightly install ninja-xtask --locked   # installs the `cargo-ninja` binary
  ```

  The repo's `.cargo/config.toml` defines the alias `stage = "ninja stage"`, so `cargo stage --strict --json` then works as documented in the `cargo-stage` skill.

## Verification

- Always verify with `cargo stage --strict --json` (see the `cargo-stage` skill); never run `cargo fmt`/`check`/`clippy`/`test` directly.
- Gotcha: with `--json` the JSON report goes to **stderr**; the exit code is the overall pass/fail signal. Parse a captured stderr file with `jaq`, e.g. `jaq -r '.[] | "\(.task): \(.status)"' /tmp/stage.json`.
- Gotcha: on success `cargo stage` also runs `git add .` — before committing, use targeted `git add <paths>` (or `git reset` first) so unrelated local-only files are never committed.
- Windows-target lint (as CI does): `rustup target add x86_64-pc-windows-msvc`, then
  `cargo clippy -p redbook --all-features --all-targets --target x86_64-pc-windows-msvc -- --deny warnings`
  Use `-p redbook`: `redbook-rip`'s build script needs `llvm-rc`, which only exists in the CI image. Windows binaries cannot execute in this Linux sandbox — see the `compilation` skill.

## Rules

- Network access to crates.io, static.rust-lang.org and deb.debian.org works; `sudo`/root does not.
- **Never modify repo files** (`.cargo/config.toml`, `Cargo.toml`, …) to make the environment work — keep everything in `/tmp` and shell exports. If a temporary local tweak is ever unavoidable, revert it before committing and use targeted `git add`.
- If the sandbox is not Debian-based or `apt-get` is unavailable, fall back to building OpenSSL from source (`openssl-src` vendored) or ask the user.
