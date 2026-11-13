#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# The file fetched from the base commit stays the base bytes when the
# working tree has already replaced them.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/fetch-from-base.sh"
root=$(mktemp -d)
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

repo="$root/repo"
mkdir -p "$repo"
git -C "$repo" init -b main >/dev/null
printf 'mode: enforce\n' >"$repo/policy.yaml"
git -C "$repo" add policy.yaml
git -C "$repo" -c user.name=bomward-test -c user.email=bomward-test@example.com commit -m base >/dev/null
base=$(git -C "$repo" rev-parse HEAD)
printf 'mode: allow-everything\n' >"$repo/policy.yaml"

dest="$root/fetched.yaml"
(
  cd "$repo"
  BOMWARD_BASE_SHA="$base" \
    BOMWARD_REPO_PATH=policy.yaml \
    BOMWARD_DEST="$dest" \
    bash "$script" >/dev/null
)
got=$(cat "$dest")
test "$got" = "mode: enforce"
test "$(cat "$repo/policy.yaml")" = "mode: allow-everything"

missing_code=0
(
  cd "$repo"
  BOMWARD_BASE_SHA="$base" \
    BOMWARD_REPO_PATH=missing.yaml \
    BOMWARD_DEST="$root/missing.yaml" \
    bash "$script" >/dev/null 2>&1
) || missing_code=$?
test "$missing_code" -ne 0
test ! -e "$root/missing.yaml"
echo "fetch-from-base: base bytes survived a dirty worktree"
