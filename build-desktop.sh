#!/bin/bash
set -euo pipefail

SCRIPT_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_ROOT"

CONFIG_OVERRIDE='{"bundle":{"macOS":{"entitlements":"./entitlements.plist.identity"}}}'
if [ "$(uname)" = "Darwin" ]; then
  VITE_APP_MODE=tauri yarn tauri build --features ai-image-search --target aarch64-apple-darwin --config "$CONFIG_OVERRIDE"
  VITE_APP_MODE=tauri yarn tauri build --features ai-image-search --target x86_64-apple-darwin --config "$CONFIG_OVERRIDE"
else
  VITE_APP_MODE=tauri yarn tauri build --features ai-image-search
fi
