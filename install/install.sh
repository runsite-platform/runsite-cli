#!/bin/sh
# RunSite CLI installer for Linux and macOS.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/runsite-platform/runsite-cli/master/install/install.sh | sh
#
# Environment overrides:
#   RUNSITE_VERSION     pin a specific version (e.g. v0.1.0). Default: latest
#   RUNSITE_INSTALL_DIR install directory. Default: $HOME/.local/bin
#   RUNSITE_REPO        GitHub repo. Default: runsite-platform/runsite-cli

set -eu

REPO="${RUNSITE_REPO:-runsite-platform/runsite-cli}"
VERSION="${RUNSITE_VERSION:-latest}"
INSTALL_DIR="${RUNSITE_INSTALL_DIR:-$HOME/.local/bin}"

err() { echo "error: $*" >&2; exit 1; }

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "'$1' is required but not installed"
}

need_cmd uname
need_cmd tar
need_cmd mktemp

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
    fetch_stdout() { curl -fsSL "$1"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
    fetch_stdout() { wget -qO- "$1"; }
else
    err "curl or wget is required"
fi

os="$(uname -s)"
case "$os" in
    Linux)  os_triple="unknown-linux-musl" ;;
    Darwin) os_triple="apple-darwin" ;;
    *)      err "unsupported OS: $os" ;;
esac

arch="$(uname -m)"
case "$arch" in
    x86_64|amd64)   arch_triple="x86_64" ;;
    arm64|aarch64)  arch_triple="aarch64" ;;
    *)              err "unsupported architecture: $arch" ;;
esac

target="${arch_triple}-${os_triple}"

if [ "$VERSION" = "latest" ]; then
    VERSION="$(fetch_stdout "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | head -n 1 \
        | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
    [ -n "$VERSION" ] || err "could not determine latest version"
fi

archive="runsite-${VERSION}-${target}.tar.gz"
base="https://github.com/${REPO}/releases/download/${VERSION}"
url="${base}/${archive}"

echo "Installing runsite ${VERSION} for ${target}"
echo "  from ${url}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT TERM

fetch "$url" "$tmpdir/$archive" || err "download failed: $url"

# Verify against the release's SHA256SUMS.txt before unpacking anything.
if command -v sha256sum >/dev/null 2>&1; then
    sum_cmd="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    sum_cmd="shasum -a 256"
else
    err "sha256sum or shasum is required to verify the download"
fi

fetch "${base}/SHA256SUMS.txt" "$tmpdir/SHA256SUMS.txt" \
    || err "could not download checksums: ${base}/SHA256SUMS.txt"

expected="$(grep " \{1,2\}\*\{0,1\}${archive}$" "$tmpdir/SHA256SUMS.txt" | awk '{print $1}')"
[ -n "$expected" ] || err "no checksum listed for ${archive}"

actual="$($sum_cmd "$tmpdir/$archive" | awk '{print $1}')"
[ "$expected" = "$actual" ] || err "checksum mismatch for ${archive}
  expected: ${expected}
  actual:   ${actual}"

tar -xzf "$tmpdir/$archive" -C "$tmpdir" || err "extract failed"
[ -f "$tmpdir/runsite" ] || err "binary not found in archive"

mkdir -p "$INSTALL_DIR" || err "cannot create $INSTALL_DIR"
mv "$tmpdir/runsite" "$INSTALL_DIR/runsite"
chmod +x "$INSTALL_DIR/runsite"

echo ""
echo "Installed: $INSTALL_DIR/runsite"

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        echo "Run: runsite --help"
        ;;
    *)
        echo ""
        echo "$INSTALL_DIR is not in your PATH. Add this to your shell profile:"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac
