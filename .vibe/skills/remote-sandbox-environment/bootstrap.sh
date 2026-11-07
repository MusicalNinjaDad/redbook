#!/bin/sh
# Bootstrap a build environment for this repo in a fresh remote Linux sandbox
# (Debian/Ubuntu-based, no root required). Idempotent.
#
# Usage:
#   sh .vibe/skills/remote-sandbox-environment/bootstrap.sh
#   . /tmp/redbook-env.sh        # in every shell that builds/tests this repo
#
# See SKILL.md in this directory for why each step exists.
set -eu

PREFIX=/tmp/local
APT=/tmp/apt
DEBS=/tmp/debs
BIN=/tmp/bin
ENVF=/tmp/redbook-env.sh

mkdir -p "$APT/lists/partial" "$APT/cache/archives/partial" "$APT/sources.list.d" \
    "$DEBS" "$PREFIX" "$BIN"
touch "$APT/status"
: > "$APT/sources.list"

# Copy the system apt sources so we can fetch package lists without root
cp /etc/apt/sources.list.d/*.sources "$APT/sources.list.d/" 2>/dev/null || true

APTOPTS="-o Dir::Etc::sourcelist=$APT/sources.list
 -o Dir::Etc::sourceparts=$APT/sources.list.d
 -o Dir::State::lists=$APT/lists
 -o Dir::State::status=$APT/status
 -o Dir::Cache=$APT/cache
 -o APT::Get::List-Cleanup=0"

apt-get $APTOPTS update

# Download (not install) the system dev packages the workspace needs:
# - libssl-dev: openssl-sys via reqwest / musicbrainz_rs (native-tls)
# - libfontconfig-dev (+freetype/expat): slint in redbook-rip (what CI installs)
# - pkgconf/libpkgconf3: pkg-config for build scripts
apt-get $APTOPTS -o Dir::Cache::archives="$DEBS" \
    install --download-only -y --no-install-recommends \
    libssl-dev libfontconfig-dev pkgconf libpkgconf3

for deb in "$DEBS"/*.deb; do
    dpkg -x "$deb" "$PREFIX"
done

# Debian keeps opensslconf.h in the multiarch include dir; openssl-sys expects
# it alongside the other OpenSSL headers
for archdir in "$PREFIX"/usr/include/*-linux-*/openssl; do
    [ -d "$archdir" ] || continue
    cp -r "$archdir/." "$PREFIX/usr/include/openssl/"
done

# pkg-config wrapper: the pkgconf binary needs its own shared lib at runtime
LIBDIR="$PREFIX/usr/lib/x86_64-linux-gnu"
printf '#!/bin/sh\nexport LD_LIBRARY_PATH=%s\nexec %s/usr/bin/pkgconf "$@"\n' "$LIBDIR" "$PREFIX" > "$BIN/pkg-config"
chmod +x "$BIN/pkg-config"

cat > "$ENVF" <<EOF
export PATH="$BIN:\$PATH"
export OPENSSL_NO_PKG_CONFIG=1
export OPENSSL_INCLUDE_DIR="$PREFIX/usr/include"
export OPENSSL_LIB_DIR="$LIBDIR"
export PKG_CONFIG_SYSROOT="$PREFIX"
export PKG_CONFIG_LIBDIR="$LIBDIR/pkgconfig:$PREFIX/usr/share/pkgconfig"
# RPATH (not RUNPATH) so transitive shared libs (e.g. libfreetype via
# libfontconfig) resolve at runtime without LD_LIBRARY_PATH
export RUSTFLAGS="-L$LIBDIR -C link-args=-Wl,--disable-new-dtags,-rpath,$LIBDIR"
EOF

echo "Bootstrap complete. In every shell that builds this repo, run:  . $ENVF"
