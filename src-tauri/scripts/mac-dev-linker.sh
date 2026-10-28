#!/usr/bin/env bash
# Wraps the default macOS linker during development builds. After the PlainApp
# dev binary links, re-signs it with the local Apple Development certificate
# and a stable identifier so the TCC screen-recording grant survives rebuilds
# (an ad-hoc identity changes cdhash on every relink; a certificate identity
# does not). Machines without the certificate — CI, other developers — keep
# the stock ad-hoc behavior.
set -u

out=""
prev=""
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    out="$arg"
  fi
  prev="$arg"
done

is_app_bin=false
case "$out" in
  */target/debug/deps/plainapp_desktop-*) is_app_bin=true ;;
esac

if [ "$is_app_bin" = true ]; then
  # Reserve signature space so codesign can replace the linker's ad-hoc
  # signature with the certificate CMS blob without relinking.
  cc "$@" -Wl,-headerpad,0x8000
else
  cc "$@"
fi
status=$?
[ $status -ne 0 ] && exit $status

if [ "$is_app_bin" = true ]; then
  identity=$(security find-identity -v -p codesigning 2>/dev/null \
    | awk 'match($0, /".*"/) { print substr($0, RSTART + 1, RLENGTH - 2); exit }')
  if [ -z "$identity" ]; then
    exit 0
  fi
  signing_output=$(codesign --force --sign "$identity" \
      --identifier com.ismartcoding.plain.desktop.dev "$out" 2>&1)
  status=$?
  if [ "$status" -ne 0 ]; then
    printf '%s\n' "$signing_output" >&2
    exit "$status"
  fi
  if [ -n "$signing_output" ] && [ "$signing_output" != "$out: replacing existing signature" ]; then
    printf '%s\n' "$signing_output" >&2
  fi
fi
exit 0
