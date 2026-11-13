#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Base and head come from the pull_request event. Without that event
# they fall back to GITHUB_BASE_REF and GITHUB_SHA.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/resolve-inputs.sh"
root=$(mktemp -d)
cleanup() { rm -rf "$root"; }
trap cleanup EXIT

event="$root/event.json"
cat >"$event" <<'EOF'
{"pull_request":{"number":12,"base":{"sha":"base123","ref":"main"},"head":{"sha":"head456"}}}
EOF

# shellcheck disable=SC1090
eval "$(
  GITHUB_EVENT_PATH="$event" \
    GITHUB_SHA=merge789 \
    GITHUB_BASE_REF=other \
    BOMWARD_SBOM=path/sbom.json \
    BOMWARD_POLICY=policy.yaml \
    bash "$script"
)"
test "$BOMWARD_BASE_SHA" = "base123"
test "$BOMWARD_HEAD_SHA" = "head456"
test "$BOMWARD_PR_NUMBER" = "12"
test "$BOMWARD_SBOM_PATH" = "path/sbom.json"
test "$BOMWARD_OLD_SBOM_PATH" = "path/sbom.json"
test "$BOMWARD_POLICY_PATH" = "policy.yaml"

# shellcheck disable=SC1090
eval "$(
  GITHUB_EVENT_PATH= \
    GITHUB_SHA=abc \
    GITHUB_BASE_REF=main \
    BOMWARD_SBOM=sbom.json \
    BOMWARD_OLD_SBOM=old.json \
    bash "$script"
)"
test "$BOMWARD_BASE_SHA" = "main"
test "$BOMWARD_HEAD_SHA" = "abc"
test "$BOMWARD_OLD_SBOM_PATH" = "old.json"
test -z "$BOMWARD_POLICY_PATH"
test -z "$BOMWARD_PR_NUMBER"

code=0
BOMWARD_SBOM=../escape.json bash "$script" >/dev/null 2>&1 || code=$?
test "$code" -ne 0
echo "resolve-inputs: event sha wins, and a path cannot escape"
