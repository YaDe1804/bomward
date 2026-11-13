#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Run bomward three times and keep each report. The exit code is the
# verdict: 0, 1, or 2. Exit 2 writes no success marker.
#
# Inputs: BOMWARD_BIN, BOMWARD_OLD, BOMWARD_NEW, BOMWARD_POLICY,
#         BOMWARD_OUT_DIR, BOMWARD_MAX_FILE_BYTES, BOMWARD_MAX_COMPONENTS,
#         BOMWARD_MAX_DEPTH, GITHUB_STEP_SUMMARY
# Stdout: BOMWARD_EXIT, BOMWARD_MARKDOWN, BOMWARD_TERMINAL, BOMWARD_JSON
set -euo pipefail

if [ -z "${BOMWARD_BIN:-}" ] || [ -z "${BOMWARD_OLD:-}" ] || [ -z "${BOMWARD_NEW:-}" ] || [ -z "${BOMWARD_OUT_DIR:-}" ]; then
  echo "error: BOMWARD_BIN, BOMWARD_OLD, BOMWARD_NEW, and BOMWARD_OUT_DIR are required" >&2
  exit 2
fi
bin=$BOMWARD_BIN
old=$BOMWARD_OLD
new=$BOMWARD_NEW
out=$BOMWARD_OUT_DIR
mkdir -p "$out"

args=("$bin" diff "$old" "$new")
if [ -n "${BOMWARD_POLICY:-}" ]; then
  args+=(--policy "$BOMWARD_POLICY")
fi
if [ -n "${BOMWARD_MAX_FILE_BYTES:-}" ]; then
  args+=(--max-file-bytes "$BOMWARD_MAX_FILE_BYTES")
fi
if [ -n "${BOMWARD_MAX_COMPONENTS:-}" ]; then
  args+=(--max-components "$BOMWARD_MAX_COMPONENTS")
fi
if [ -n "${BOMWARD_MAX_DEPTH:-}" ]; then
  args+=(--max-depth "$BOMWARD_MAX_DEPTH")
fi

run_format_code=0
run_format() {
  local format=$1
  local dest=$2
  set +e
  "${args[@]}" --format "$format" --output "$dest"
  run_format_code=$?
  set -e
  if [ "$run_format_code" -eq 2 ]; then
    echo "error: bomward failed before a verdict" >&2
    exit 2
  fi
}

run_format markdown "${out}/report.md"
verdict=$run_format_code
run_format terminal "${out}/report.txt"
term=$run_format_code
run_format json "${out}/report.json"
json=$run_format_code
if [ "$term" != "$verdict" ] || [ "$json" != "$verdict" ]; then
  echo "error: bomward exit codes disagreed (${verdict}, ${term}, ${json})" >&2
  exit 2
fi
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  cat "${out}/report.txt" >>"${GITHUB_STEP_SUMMARY}"
fi
printf 'BOMWARD_EXIT=%q\n' "$verdict"
printf 'BOMWARD_MARKDOWN=%q\n' "${out}/report.md"
printf 'BOMWARD_TERMINAL=%q\n' "${out}/report.txt"
printf 'BOMWARD_JSON=%q\n' "${out}/report.json"
exit "$verdict"
