#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# An existing bomward comment is replaced. A missing write permission
# still exits 0 and copies the report into the job summary.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/upsert-comment.sh"
root=$(mktemp -d)
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

comments="$root/comments"
mkdir -p "$comments"
printf 'hello from someone else\n' >"$comments/01-other.body"
printf '<!-- bomward-review -->\nold report\n' >"$comments/02-bomward.body"
body="$root/new.md"
printf 'new report line\n' >"$body"

BOMWARD_COMMENTS_DIR="$comments" \
  BOMWARD_BODY_FILE="$body" \
  BOMWARD_CAN_COMMENT=true \
  bash "$script" >/dev/null

count=$(find "$comments" -name '*.body' | wc -l | tr -d ' ')
test "$count" = "2"
test "$(cat "$comments/01-other.body")" = "hello from someone else"
grep -q "new report line" "$comments/02-bomward.body"
grep -q "bomward-review" "$comments/02-bomward.body"
if grep -q "old report" "$comments/02-bomward.body"; then
  echo "old comment body was kept" >&2
  exit 1
fi

snapshot="$root/snapshot"
mkdir -p "$snapshot"
cp "$comments"/*.body "$snapshot/"
summary="$root/summary"
: >"$summary"
BOMWARD_COMMENTS_DIR="$comments" \
  BOMWARD_BODY_FILE="$body" \
  BOMWARD_CAN_COMMENT=false \
  GITHUB_STEP_SUMMARY="$summary" \
  bash "$script" >/dev/null
test "$(cat "$summary")" != ""
grep -q "new report line" "$summary"
cmp -s "$comments/01-other.body" "$snapshot/01-other.body"
cmp -s "$comments/02-bomward.body" "$snapshot/02-bomward.body"
count=$(find "$comments" -name '*.body' | wc -l | tr -d ' ')
test "$count" = "2"
echo "upsert-comment: one comment updated, no-write exits 0"
