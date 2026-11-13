#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Update the pull-request comment that carries the bomward marker, or
# create it. A missing write permission writes the report to the job
# summary and exits 0.
#
# Inputs: BOMWARD_BODY_FILE, BOMWARD_MARKER, BOMWARD_CAN_COMMENT,
#         BOMWARD_COMMENTS_DIR (simulated comment list), BOMWARD_PR,
#         GITHUB_REPOSITORY, GITHUB_STEP_SUMMARY
set -euo pipefail

marker=${BOMWARD_MARKER:-<!-- bomward-review -->}
body_file=${BOMWARD_BODY_FILE:?}
body=$(cat "$body_file")
case "$body" in
  *"$marker"*) payload=$body ;;
  *) payload="${marker}"$'\n'"${body}" ;;
esac

write_summary() {
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf '%s\n' "$payload" >>"${GITHUB_STEP_SUMMARY}"
  else
    printf '%s\n' "$payload"
  fi
}

permission_denied() {
  grep -q -E 'HTTP 403|Resource not accessible' "$1"
}

if [ "${BOMWARD_CAN_COMMENT:-true}" != "true" ]; then
  write_summary
  exit 0
fi

if [ -n "${BOMWARD_COMMENTS_DIR:-}" ]; then
  found=""
  shopt -s nullglob
  for file in "${BOMWARD_COMMENTS_DIR}"/*.body; do
    if grep -q -F -- "$marker" "$file"; then
      found=$file
      break
    fi
  done
  shopt -u nullglob
  if [ -n "$found" ]; then
    printf '%s\n' "$payload" >"$found"
  else
    printf '%s\n' "$payload" >"${BOMWARD_COMMENTS_DIR}/new.body"
  fi
  exit 0
fi

pr=${BOMWARD_PR:-}
if [ -z "$pr" ]; then
  write_summary
  exit 0
fi
repository=${GITHUB_REPOSITORY:?}
err_file=$(mktemp)

set +e
id=$(
  gh api --paginate "repos/${repository}/issues/${pr}/comments" \
    --jq "map(select(.body != null and (.body | contains(\"${marker}\")))) | .[0].id // empty" \
    2>"$err_file"
)
api_code=$?
set -e
if [ "$api_code" -ne 0 ]; then
  if permission_denied "$err_file"; then
    write_summary
    exit 0
  fi
  cat "$err_file" >&2
  exit "$api_code"
fi
id=$(printf '%s\n' "$id" | head -n 1 | tr -d '[:space:]')

set +e
if [ -z "$id" ] || [ "$id" = "null" ] || [ "$id" = "empty" ]; then
  gh api --method POST "repos/${repository}/issues/${pr}/comments" \
    --raw-field body="$payload" >/dev/null 2>"$err_file"
  api_code=$?
else
  gh api --method PATCH "repos/${repository}/issues/comments/${id}" \
    --raw-field body="$payload" >/dev/null 2>"$err_file"
  api_code=$?
fi
set -e
if [ "$api_code" -ne 0 ]; then
  if permission_denied "$err_file"; then
    write_summary
    exit 0
  fi
  cat "$err_file" >&2
  exit "$api_code"
fi
