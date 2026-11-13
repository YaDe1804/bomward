#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fail=0
for test_name in fetch-from-base.sh install-bomward.sh install-local.sh upsert-comment.sh resolve-inputs.sh; do
  echo "== $test_name"
  if bash "$here/$test_name"; then
    echo "ok $test_name"
  else
    echo "FAIL $test_name" >&2
    fail=1
  fi
done
exit "$fail"
