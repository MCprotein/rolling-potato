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

    case "$line" in
      *$'\r'*)
        fail "checksum lines must use LF endings: $checksum_file"
        ;;
    esac

    set -- $line
    if [ "$#" -ne 2 ]; then
      fail "checksum line must have exactly hash and basename fields: $checksum_file"
    fi

    checksum="$1"
    if [ "${#checksum}" -ne 64 ]; then
      fail "checksum hash must contain exactly 64 hexadecimal characters: $checksum_file"
    fi
    case "$checksum" in
      *[!0-9A-Fa-f]*)
        fail "checksum hash must be hexadecimal: $checksum_file"
        ;;
    esac

    asset_name="$2"
    case "$asset_name" in
      */* | *\\*)
        fail "checksum path must be a release asset basename, got: $asset_name"
        ;;
    esac
  done < "$checksum_file"
done
