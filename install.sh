#!/bin/sh
# Install dodo — keyboard-first todo + time tracker CLI
# Usage: curl -fsSL https://raw.githubusercontent.com/oksure/dodo/main/install.sh | sh
set -e

REPO="oksure/dodo"
INSTALL_DIR="${DODO_INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and arch
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    TARGET="universal-apple-darwin"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    TARGET="x86_64-pc-windows-msvc"
    ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

# Get latest release tag
TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
if [ -z "$TAG" ]; then
  echo "Failed to fetch latest release" >&2
  exit 1
fi

echo "Installing dodo $TAG for $TARGET..."

# Download and extract
URL="https://github.com/$REPO/releases/download/$TAG/dodo-$TARGET.tar.gz"
mkdir -p "$INSTALL_DIR"

if command -v tar >/dev/null 2>&1; then
  curl -fsSL "$URL" | tar xz -C "$INSTALL_DIR"
else
  echo "tar is required" >&2
  exit 1
fi

chmod +x "$INSTALL_DIR/dodo"
echo "Installed dodo to $INSTALL_DIR/dodo"

# Check if install dir is in PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add $INSTALL_DIR to your PATH:"; echo "  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac

echo "Run 'dodo' to get started."
