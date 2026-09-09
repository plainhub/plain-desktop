#!/bin/bash
set -euo pipefail

SCRIPT_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_ROOT"

CONFIG_OVERRIDE='{"bundle":{"macOS":{"entitlements":"./entitlements.plist.identity"}}}'
if [ "$(uname)" = "Darwin" ]; then
  VITE_APP_MODE=tauri yarn tauri build --target aarch64-apple-darwin --config "$CONFIG_OVERRIDE" -- --locked
  VITE_APP_MODE=tauri yarn tauri build --target x86_64-apple-darwin --config "$CONFIG_OVERRIDE" -- --locked
else
  VITE_APP_MODE=tauri yarn tauri build -- --locked
fi
