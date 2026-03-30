#!/bin/sh
set -e

REPO="cameronrothenburg/roca"
INSTALL_DIR="${ROCA_INSTALL_DIR:-/usr/local/bin}"

# Get latest release tag
TAG=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$TAG" ]; then
    echo "error: could not fetch latest release"
    exit 1
fi

echo "Installing roca ${TAG}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -sL "https://github.com/${REPO}/releases/download/${TAG}/roca" -o "${TMP}/roca"
chmod +x "${TMP}/roca"

# Verify it runs
if ! "${TMP}/roca" --version >/dev/null 2>&1; then
    echo "error: downloaded binary is not compatible with this system"
    exit 1
fi

# Install
if [ -w "$INSTALL_DIR" ]; then
    mv "${TMP}/roca" "${INSTALL_DIR}/roca"
else
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "${TMP}/roca" "${INSTALL_DIR}/roca"
fi

echo "roca ${TAG} installed to ${INSTALL_DIR}/roca"
