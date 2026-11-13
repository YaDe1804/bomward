#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Download the pinned release asset and check SHA256SUMS before the
# binary is made executable. The binary is not started.
#
# Inputs: BOMWARD_VERSION, BOMWARD_REPOSITORY, RUNNER_OS,
#         BOMWARD_RELEASE_DIR (local asset directory, skips the download)
# Stdout: BOMWARD_BIN=<path>
# Exit 2 when the checksum does not match.
set -euo pipefail

version=${BOMWARD_VERSION:-v0.1.0}
repository=${BOMWARD_REPOSITORY:-YaDe1804/bomward}

select_asset() {
  case "${RUNNER_OS:-}" in
    Linux)
      asset="bomward-x86_64-unknown-linux-gnu.tar.gz"
      bin="bomward"
      ;;
    macOS)
      asset="bomward-aarch64-apple-darwin.tar.gz"
      bin="bomward"
      ;;
    Windows)
      asset="bomward-x86_64-pc-windows-msvc.zip"
      bin="bomward.exe"
      ;;
    *)
      echo "error: unsupported runner OS ${RUNNER_OS:-unknown}" >&2
      exit 2
      ;;
  esac
}

if [ -z "${RUNNER_OS:-}" ]; then
  uname_s=$(uname -s)
  case "$uname_s" in
    Linux) RUNNER_OS=Linux ;;
    Darwin) RUNNER_OS=macOS ;;
    MINGW* | MSYS* | CYGWIN*) RUNNER_OS=Windows ;;
    *)
      echo "error: unsupported runner ${uname_s}" >&2
      exit 2
      ;;
  esac
fi
select_asset

if [ -n "${BOMWARD_RELEASE_DIR:-}" ]; then
  release=${BOMWARD_RELEASE_DIR}
else
  release=$(mktemp -d)
  gh release download "$version" -R "$repository" -p "$asset" -p SHA256SUMS -D "$release"
fi

sums="${release}/SHA256SUMS"
file="${release}/${asset}"
if [ ! -f "$sums" ] || [ ! -f "$file" ]; then
  echo "error: release ${version} is missing ${asset} or SHA256SUMS" >&2
  exit 2
fi

expected=$(
  awk -v asset="$asset" '
    {
      name = $2
      sub(/^\.\//, "", name)
      if (name == asset) print $1
    }
  ' "$sums" | tail -n 1
)
if [ -z "$expected" ]; then
  echo "error: SHA256SUMS has no entry for ${asset}" >&2
  exit 2
fi

if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$file" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$file" | awk '{print $1}')
else
  echo "error: sha256sum is not available" >&2
  exit 2
fi
actual=$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')
expected=$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')
if [ "$actual" != "$expected" ]; then
  echo "error: checksum mismatch for ${asset}" >&2
  exit 2
fi

work=$(mktemp -d)
case "$asset" in
  *.tar.gz) tar -xzf "$file" -C "$work" ;;
  *.zip) unzip -o "$file" -d "$work" >/dev/null ;;
  *)
    echo "error: unknown asset type ${asset}" >&2
    exit 2
    ;;
esac
found=$(find "$work" -type f -name "$bin" | head -n 1)
if [ -z "$found" ]; then
  echo "error: ${asset} does not contain ${bin}" >&2
  exit 2
fi
chmod +x "$found" 2>/dev/null || true
printf 'BOMWARD_BIN=%q\n' "$found"
