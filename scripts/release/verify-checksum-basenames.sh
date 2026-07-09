#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'checksum basename error: %s\n' "$1" >&2
  exit 1
}

if [ "$#" -lt 1 ]; then
  fail "usage: scripts/release/verify-checksum-basenames.sh <checksum-file>..."
fi

for checksum_file in "$@"; do
  if [ ! -f "$checksum_file" ]; then
    fail "checksum file was not found: $checksum_file"
  fi

  while IFS= read -r line || [ -n "$line" ]; do
    [ -z "$line" ] && continue

    set -- $line
    if [ "$#" -ne 2 ]; then
      fail "checksum line must have exactly hash and basename fields: $checksum_file"
    fi

    asset_name="$2"
    case "$asset_name" in
      */* | *\\*)
        fail "checksum path must be a release asset basename, got: $asset_name"
        ;;
    esac
  done < "$checksum_file"
done
