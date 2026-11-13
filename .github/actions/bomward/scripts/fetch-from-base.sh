#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Copy one file from the base commit. The working tree is not read.
#
# Inputs: BOMWARD_BASE_SHA, BOMWARD_REPO_PATH, BOMWARD_DEST
# Stdout: BOMWARD_FETCHED=<path>
# Exit 2 when the commit or the file is missing. The destination is
# left untouched in that case.
set -euo pipefail

if [ -z "${BOMWARD_BASE_SHA:-}" ] || [ -z "${BOMWARD_REPO_PATH:-}" ] || [ -z "${BOMWARD_DEST:-}" ]; then
  echo "error: BOMWARD_BASE_SHA, BOMWARD_REPO_PATH, and BOMWARD_DEST are required" >&2
  exit 2
fi
base=$BOMWARD_BASE_SHA
path=$BOMWARD_REPO_PATH
dest=$BOMWARD_DEST

if ! git rev-parse --verify "${base}^{commit}" >/dev/null 2>&1; then
  if git rev-parse --verify "origin/${base}^{commit}" >/dev/null 2>&1; then
    base="origin/${base}"
  else
    echo "error: base commit ${BOMWARD_BASE_SHA} is not in this repository; check out with fetch-depth: 0" >&2
    exit 2
  fi
fi

partial="${dest}.partial"
rm -f "$partial"
if ! git show "${base}:${path}" >"$partial" 2>/dev/null; then
  rm -f "$partial"
  echo "error: ${path} is not in ${base}" >&2
  exit 2
fi
mv "$partial" "$dest"
printf 'BOMWARD_FETCHED=%q\n' "$dest"
