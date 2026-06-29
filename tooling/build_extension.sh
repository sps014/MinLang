#!/usr/bin/env bash
# Builds and packages the Dream VS Code extension (.vsix)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VSCODE_DIR="$SCRIPT_DIR/vscode"

echo "==> Navigating to VS Code extension directory..."
cd "$VSCODE_DIR"

echo "==> Installing dependencies..."
npm install

echo "==> Compiling TypeScript..."
npm run compile

echo "==> Packaging extension into .vsix..."
npx @vscode/vsce package --no-dependencies

echo "==> Done! You can install the extension with:"
echo "    code --install-extension tooling/vscode/$(ls *.vsix | head -n 1)"
