#!/usr/bin/env bash
set -euo pipefail

for binary in "$@"; do
  readelf -W --version-info "$binary" | awk -v binary="$binary" '
    /^Version needs section/ { needs = 1; next }
    /^Version .* section/ { needs = 0 }
    needs {
      for (i = 1; i < NF; i++) {
        if ($i != "Name:" || $(i + 1) !~ /^GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?$/) continue
        found = 1
        version = $(i + 1)
        split(substr(version, 7), parts, ".")
        if (parts[1] > 2 || (parts[1] == 2 && (parts[2] > 39 || (parts[2] == 39 && parts[3] > 0)))) {
          printf "%s requires %s (Ubuntu 24.04 baseline is GLIBC_2.39)\n", binary, version > "/dev/stderr"
          failed = 1
        }
      }
    }
    END {
      if (!found) printf "%s: no glibc version requirements found\n", binary > "/dev/stderr"
      if (!found || failed) exit 1
    }
  '
done
