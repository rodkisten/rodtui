#!/bin/sh
# rodtui release installer.
# This script installs only published GitHub release artifacts.
# It does not compile source code or execute downloaded programs during installation.
set -eu

REPO="rodkisten/rodtui"
VERSION="${RODTUI_VERSION:-latest}"
BIN_DIR="${RODTUI_INSTALL_DIR:-${HOME}/.local/bin}"

fail() { printf 'rodtui installer: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"; }

need curl
need tar
need mktemp

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Linux) TARGET_OS="unknown-linux-musl" ;;
  Darwin) TARGET_OS="apple-darwin" ;;
  *) fail "unsupported OS: $OS (use the release assets for other platforms)" ;;
esac
case "$ARCH" in
  x86_64|amd64) TARGET_ARCH="x86_64" ;;
  aarch64|arm64) TARGET_ARCH="aarch64" ;;
  *) fail "unsupported architecture: $ARCH" ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"
ASSET="rodtui-${TARGET}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  BASE="https://github.com/${REPO}/releases/latest/download"
else
  # Only allow conventional version tags to prevent path/URL injection.
  case "$VERSION" in *[!a-zA-Z0-9._-]*|'') fail "invalid version tag" ;; esac
  BASE="https://github.com/${REPO}/releases/download/${VERSION}"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
curl -fLsS --retry 2 "${BASE}/${ASSET}" -o "${TMP}/${ASSET}" || fail "release archive not available: ${ASSET}"
curl -fLsS --retry 2 "${BASE}/SHA256SUMS" -o "${TMP}/SHA256SUMS" || fail "release checksum manifest not available"

EXPECTED="$(awk -v name="$ASSET" '$2 == name {print $1}' "${TMP}/SHA256SUMS")"
[ -n "$EXPECTED" ] || fail "asset not found in checksum manifest"
case "$EXPECTED" in *[!a-fA-F0-9]*|'') fail "invalid checksum" ;; esac
[ "${#EXPECTED}" -eq 64 ] || fail "invalid SHA-256 checksum length"

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "${TMP}/${ASSET}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "${TMP}/${ASSET}" | awk '{print $1}')"
else
  fail "sha256sum or shasum is required"
fi
[ "$ACTUAL" = "$EXPECTED" ] || fail "SHA-256 verification failed"

# The archive is produced by our release workflow with a single binary entry.
# Validate the member list before extraction to reject path traversal and extras.
MEMBERS="$(tar -tzf "${TMP}/${ASSET}")" || fail "invalid release archive"
[ "$MEMBERS" = "rodtui" ] || fail "unexpected archive contents"
tar -xzf "${TMP}/${ASSET}" -C "$TMP" || fail "failed to extract archive"
[ -f "${TMP}/rodtui" ] || fail "binary missing from archive"

mkdir -p "$BIN_DIR"
chmod 755 "${TMP}/rodtui"
mv "${TMP}/rodtui" "${BIN_DIR}/rodtui" || fail "installation failed"
printf 'Installed rodtui to %s/rodtui\n' "$BIN_DIR"
case ":${PATH}:" in *":${BIN_DIR}:"*) ;; *) printf 'Add %s to PATH if needed.\n' "$BIN_DIR" ;; esac
