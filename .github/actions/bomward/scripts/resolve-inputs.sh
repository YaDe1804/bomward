#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Resolve the base commit, the head commit, and the repository paths.
# The pull_request event wins. Otherwise the base is GITHUB_BASE_REF and
# the head is GITHUB_SHA.
#
# Inputs: GITHUB_EVENT_PATH, GITHUB_SHA, GITHUB_BASE_REF, BOMWARD_SBOM,
#         BOMWARD_OLD_SBOM, BOMWARD_POLICY
# Stdout: shell assignments for BOMWARD_BASE_SHA, BOMWARD_HEAD_SHA,
#         BOMWARD_PR_NUMBER, BOMWARD_SBOM_PATH, BOMWARD_OLD_SBOM_PATH,
#         BOMWARD_POLICY_PATH
set -euo pipefail

reject_path() {
  local label=$1
  local value=$2
  local part
  case "$value" in
    "" | /* | \\* | [A-Za-z]:*)
      echo "error: ${label} must be a relative path inside the repository" >&2
      exit 2
      ;;
  esac
  local IFS=/
  # The split is the path separator, not a glob.
  # shellcheck disable=SC2086
  for part in $value; do
    if [ -z "$part" ] || [ "$part" = "." ] || [ "$part" = ".." ]; then
      echo "error: ${label} must stay inside the repository" >&2
      exit 2
    fi
  done
}

base=""
head=""
number=""
if [ -n "${GITHUB_EVENT_PATH:-}" ] && [ -f "${GITHUB_EVENT_PATH}" ]; then
  py=""
  if python3 -c "import sys" >/dev/null 2>&1; then
    py=python3
  elif python -c "import sys" >/dev/null 2>&1; then
    py=python
  else
    echo "error: python is required to read the GitHub event" >&2
    exit 2
  fi
  event_path=${GITHUB_EVENT_PATH}
  if command -v cygpath >/dev/null 2>&1; then
    event_path=$(cygpath -w "$event_path")
  fi
  parsed=$("$py" - "$event_path" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
pull_request = data.get("pull_request") or {}
base_sha = (pull_request.get("base") or {}).get("sha") or ""
head_sha = (pull_request.get("head") or {}).get("sha") or ""
pr_number = pull_request.get("number")
pr_number = "" if pr_number is None else str(pr_number)
sys.stdout.write(base_sha + "\n" + head_sha + "\n" + pr_number + "\n")
PY
)
  parsed=${parsed//$'\r'/}
  base=${parsed%%$'\n'*}
  rest=${parsed#*$'\n'}
  head=${rest%%$'\n'*}
  number=${rest#*$'\n'}
  number=${number%$'\n'}
fi

if [ -z "$base" ]; then
  base=${GITHUB_BASE_REF:-}
fi
if [ -z "$head" ]; then
  head=${GITHUB_SHA:-}
fi
if [ -z "$base" ] || [ -z "$head" ]; then
  echo "error: the base commit and the head commit are unknown" >&2
  exit 2
fi

sbom=${BOMWARD_SBOM:-}
reject_path sbom "$sbom"
old=${BOMWARD_OLD_SBOM:-}
if [ -z "$old" ]; then
  old=$sbom
else
  reject_path old-sbom "$old"
fi
policy=${BOMWARD_POLICY:-}
if [ -n "$policy" ]; then
  reject_path policy "$policy"
fi

printf 'BOMWARD_BASE_SHA=%q\n' "$base"
printf 'BOMWARD_HEAD_SHA=%q\n' "$head"
printf 'BOMWARD_PR_NUMBER=%q\n' "$number"
printf 'BOMWARD_SBOM_PATH=%q\n' "$sbom"
printf 'BOMWARD_OLD_SBOM_PATH=%q\n' "$old"
printf 'BOMWARD_POLICY_PATH=%q\n' "$policy"
