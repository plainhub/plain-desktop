#!/bin/bash
set -euo pipefail

SCRIPT_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_ROOT"

core="$SCRIPT_ROOT/../plain-core"
revision=$(sed -n 's|^source = "git+https://github.com/plainhub/plain-core.git?branch=main#\([0-9a-f]*\)"$|\1|p' Cargo.lock | sort -u)
if [ "$(git -C "$core" rev-parse HEAD)" != "$revision" ]; then
  echo "Desktop release requires the plain-core revision locked in Cargo.lock: $revision" >&2
  exit 1
fi

python=python3
if ! "$python" -c 'import sys; sys.exit(sys.version_info < (3, 10))' 2>/dev/null; then
  python=python
fi

build() {
  local target=$1
  shift
  local relative="target/desktop-release-runtime/$target"
  local runtime digest config
  runtime=$(node -p "require('node:path').resolve(process.argv[1])" "$relative")
  "$python" "$core/plain-libs/native/ai_image_search/prepare_onnxruntime.py" --target "$target" --output "$runtime"
  digest=$(node -p "require(process.argv[1]).sha256" "$runtime/runtime.json")
  config=$(node --input-type=module -e 'import { desktopReleaseConfig } from "./scripts/desktop-release-config.mjs"; console.log(JSON.stringify(desktopReleaseConfig(process.argv[1])))' "$target")
  VITE_APP_MODE=tauri PLAIN_ONNX_RUNTIME_DIR="$runtime" PLAIN_ONNX_RUNTIME_SHA256="$digest" \
    yarn tauri build --features ai-image-search --config "$config" "$@" -- --locked
}

if [ "$(uname)" = "Darwin" ]; then
  build aarch64-apple-darwin --target aarch64-apple-darwin
  build x86_64-apple-darwin --target x86_64-apple-darwin
else
  build "$(rustc -vV | sed -n 's/^host: //p')"
fi
