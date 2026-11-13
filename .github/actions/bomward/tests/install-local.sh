#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Local mode builds the binary in this repo and never touches a release
# asset, even when that asset is present and its checksum is wrong.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/install-bomward.sh"
root=$(mktemp -d)
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

mkdir -p "$root/release" "$root/bin" "$root/src"
asset="bomward-x86_64-unknown-linux-gnu.tar.gz"
ran="$root/executed"
cat >"$root/release/$asset" <<EOF
#!/bin/sh
echo ran >"$ran"
EOF
printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' "$asset" >"$root/release/SHA256SUMS"
cat >"$root/bin/cargo" <<EOF
#!/bin/sh
mkdir -p target/release
printf 'ok' > target/release/bomward
printf 'ok' > target/release/bomward.exe
EOF
chmod +x "$root/bin/cargo"

PATH="$root/bin:${PATH}" \
  RUNNER_OS=Linux \
  BOMWARD_USE_LOCAL=1 \
  BOMWARD_LOCAL_ROOT="$root/src" \
  BOMWARD_RELEASE_DIR="$root/release" \
  bash "$script" >"$root/out"

# shellcheck disable=SC1090
eval "$(cat "$root/out")"
test ! -e "$ran"
test "$BOMWARD_BIN" = "$root/src/target/release/bomward"
test "$(cat "$BOMWARD_BIN")" = "ok"
echo "install-bomward: local mode skips the release asset"
