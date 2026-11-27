#!/bin/bash
set -euo pipefail

SCRIPT_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_ROOT"

build() {
  VITE_APP_MODE=tauri yarn tauri build "$@" -- --locked
}

if [ "$(uname)" = "Darwin" ]; then
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    build --target "$target" --config '{"bundle":{"macOS":{"entitlements":"./entitlements.plist.identity"}}}'
  done
else
  build
fi
