#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# A release asset whose digest does not match SHA256SUMS is not executed.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/install-bomward.sh"
root=$(mktemp -d)
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

asset="bomward-x86_64-unknown-linux-gnu.tar.gz"
ran="$root/executed"
cat >"$root/$asset" <<EOF
#!/bin/sh
echo ran >"$ran"
EOF
printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$asset" >"$root/SHA256SUMS"

code=0
RUNNER_OS=Linux \
  BOMWARD_RELEASE_DIR="$root" \
  BOMWARD_VERSION=v0.1.0 \
  bash "$script" >/dev/null 2>"$root/err" || code=$?
test "$code" -ne 0
test ! -e "$ran"
grep -q "checksum" "$root/err"
echo "install-bomward: a bad checksum did not run the asset"
