#!/usr/bin/env bash
# Builds and packages the Dream VS Code extension (.vsix)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VSCODE_DIR="$SCRIPT_DIR/vscode"

echo "==> Building dream-lsp native binary in release mode..."
cd "$ROOT_DIR"
cargo build --release -p dream-lsp

echo "==> Copying binary into extension folder..."
mkdir -p "$VSCODE_DIR/bin"

# Determine Node-compatible platform string
PLATFORM=""
case "$(uname -s)" in
    Darwin*) PLATFORM="darwin" ;;
    Linux*) PLATFORM="linux" ;;
    MINGW*|CYGWIN*|MSYS*) PLATFORM="win32" ;;
    *) PLATFORM="unknown" ;;
esac

# Determine Node-compatible arch string
ARCH=""
case "$(uname -m)" in
    x86_64) ARCH="x64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *) ARCH="unknown" ;;
esac

EXT=""
if [ "$PLATFORM" == "win32" ]; then
    EXT=".exe"
fi

echo "Detected Platform: $PLATFORM, Arch: $ARCH"

cp "target/release/dream-lsp$EXT" "$VSCODE_DIR/bin/dream-lsp$EXT"
cp "target/release/dream-lsp$EXT" "$VSCODE_DIR/bin/dream-lsp-${PLATFORM}-${ARCH}${EXT}"

echo "==> Navigating to VS Code extension directory..."
cd "$VSCODE_DIR"

echo "==> Installing dependencies..."
npm install

echo "==> Compiling TypeScript..."
npm run compile

echo "==> Packaging extension into .vsix..."
npx @vscode/vsce package

echo "==> Done! You can install the extension with:"
echo "    code --install-extension tooling/vscode/$(ls *.vsix | head -n 1)"
