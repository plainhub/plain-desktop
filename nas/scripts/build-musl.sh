#!/usr/bin/env bash
#
# Build a fully static binary using the musl toolchain.
#
# Prerequisites:
#   - musl-tools   (sudo apt-get install musl-tools)
#   - rustup target add x86_64-unknown-linux-musl
#
# The resulting binary is placed in:
#   target/x86_64-unknown-linux-musl/release/plain-nas

set -euo pipefail

TARGET="x86_64-unknown-linux-musl"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# ── Preflight checks ──────────────────────────────────────────────
if ! command -v musl-gcc &>/dev/null; then
    echo "ERROR: musl-gcc not found. Install with:"
    echo "  sudo apt-get install musl-tools"
    exit 1
fi

if ! rustup target list --installed | grep -q "$TARGET"; then
    echo "ERROR: musl target not installed. Run:"
    echo "  rustup target add $TARGET"
    exit 1
fi

# ── Build ──────────────────────────────────────────────────────────
echo "==> Building static binary for $TARGET ..."
cargo build \
    --release \
    --target "$TARGET" \
    "$@"

BINARY="$PROJECT_DIR/target/$TARGET/release/plain-nas"

if [[ -f "$BINARY" ]]; then
    echo ""
    echo "==> Build successful!"
    echo "    Binary: $BINARY"
    echo "    Size:   $(du -h "$BINARY" | cut -f1)"
    echo "    Type:   $(file "$BINARY" | cut -d: -f2-)"
    echo ""
    echo "    Verify static linking:"
    echo "      ldd $BINARY"
    echo "      file $BINARY"
else
    echo "ERROR: Build failed — binary not found at $BINARY"
    exit 1
fi
