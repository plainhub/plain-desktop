#!/bin/bash
set -euo pipefail

# App Store build: tauri build (app bundle) -> sign with App Store cert ->
# embed provisioning profile -> .pkg (installer-signed) -> upload via altool.
#
# Required env:
#   APPLE_APP_IDENTITY        App Store app cert (default prefix "3rd Party Mac Developer Application:")
#   APPLE_INSTALLER_IDENTITY  App Store installer cert (default prefix "3rd Party Mac Developer Installer:")
#   APPLE_PROFILE_PATH        path to the downloaded macOS App Store .provisionprofile
#   APPLE_ID / APPLE_PASSWORD  Altool credentials (app-specific password); needed for upload
#   APPLE_TEAM_ID             Team ID (--asc-provider hint for altool)
# Optional:
#   ARCHS      default "aarch64-apple-darwin x86_64-apple-darwin"
#   SKIP_UPLOAD=1  build + package only

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

ARCHS="${ARCHS:-aarch64-apple-darwin x86_64-apple-darwin}"
APP_IDENTITY="${APPLE_APP_IDENTITY:-3rd Party Mac Developer Application:}"
INSTALLER_IDENTITY="${APPLE_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer:}"
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"

[ -n "${APPLE_PROFILE_PATH:-}" ] || { echo "::error::APPLE_PROFILE_PATH is required"; exit 1; }
[ -f "$APPLE_PROFILE_PATH" ] || { echo "::error::provisioning profile not found: $APPLE_PROFILE_PATH"; exit 1; }

sdk_major="$(xcrun --sdk macosx --show-sdk-version | cut -d. -f1)"
if [ "${sdk_major:-0}" -gt 15 ]; then
  echo "::error::macOS SDK $sdk_major stamps Runtime Version > 15; App Store devices on macOS 15 would be killed on launch. Select Xcode with the macOS 15 SDK (xcode-select -s)."
  exit 1
fi

CONFIG_JSON="$(node -e '
  const id = process.argv[1];
  process.stdout.write(JSON.stringify({ bundle: { targets: ["app"], macOS: { entitlements: "./entitlements.plist", signingIdentity: id } } }));
' "$APP_IDENTITY")"

OUT_DIR="$(pwd)/target/appstore"
mkdir -p "$OUT_DIR"

for triple in $ARCHS; do
  VITE_APP_MODE=tauri yarn tauri build --target "$triple" --config "$CONFIG_JSON" -- --locked

  APP="src-tauri/target/$triple/release/bundle/macos/PlainApp.app"
  [ -d "$APP" ] || { echo "::error::missing bundle: $APP"; exit 1; }

  # Embed the App Store provisioning profile (carries the multicast entitlement)
  # and re-sign the bundle so the profile is honored.
  cp "$APPLE_PROFILE_PATH" "$APP/Contents/embedded.provisionprofile"
  codesign --force --options runtime --entitlements "$ROOT/src-tauri/entitlements.plist" --sign "$APP_IDENTITY" "$APP"
  codesign --verify --deep --strict "$APP"
  codesign -d --entitlements :- "$APP" >/dev/null

  # Installer-signed .pkg
  PKG="$OUT_DIR/PlainApp-${VERSION}-${triple%%-*}.pkg"
  productbuild --component "$APP" /Applications --sign "$INSTALLER_IDENTITY" "$PKG"
  pkgutil --check-signature "$PKG" >/dev/null

  echo "Built: $PKG"

  if [ "${SKIP_UPLOAD:-0}" != "1" ]; then
    if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_PASSWORD:-}" ]; then
      echo "::warning::APPLE_ID/APPLE_PASSWORD missing — not uploading $PKG (upload with Transporter or xcrun altool manually)."
    else
      xcrun altool --upload-package "$PKG" --type macos --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" ${APPLE_TEAM_ID:+--asc-provider "$APPLE_TEAM_ID"}
    fi
  fi
done
