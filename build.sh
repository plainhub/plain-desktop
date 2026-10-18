#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"

test -f ../plain-core/plain-server/Cargo.toml
VITE_APP_API_HOST="" yarn build --outDir ../plain-core/plain-cli/src/web_dist --emptyOutDir
