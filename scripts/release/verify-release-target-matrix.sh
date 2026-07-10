#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release target matrix error: %s\n' "$1" >&2
  exit 1
}

workflow=".github/workflows/release-binaries.yml"

if [ ! -f "$workflow" ]; then
  fail "workflow file was not found: $workflow"
fi

expect_entry() {
  os="$1"
  target="$2"
  binary="$3"
  archive="$4"

  grep -F "os: $os" "$workflow" >/dev/null \
    || fail "missing runner label: $os"
  grep -F "target: $target" "$workflow" >/dev/null \
    || fail "missing Rust target: $target"
  grep -F "binary: $binary" "$workflow" >/dev/null \
    || fail "missing binary name: $binary"
  grep -F "archive: $archive" "$workflow" >/dev/null \
    || fail "missing archive format: $archive"
}

expect_entry "macos-14" "aarch64-apple-darwin" "rpotato" "tar.gz"
expect_entry "macos-15-intel" "x86_64-apple-darwin" "rpotato" "tar.gz"
expect_entry "ubuntu-24.04" "x86_64-unknown-linux-gnu" "rpotato" "tar.gz"
expect_entry "ubuntu-24.04-arm" "aarch64-unknown-linux-gnu" "rpotato" "tar.gz"
expect_entry "windows-latest" "x86_64-pc-windows-msvc" "rpotato.exe" "zip"

grep -F "name: Package tar.gz archive" "$workflow" >/dev/null \
  || fail "tar.gz packaging step must stay OS-neutral"
grep -F "name: Package Windows archive" "$workflow" >/dev/null \
  || fail "Windows zip packaging step is missing"
grep -F "scripts/release/verify-checksum-basenames.sh" "$workflow" >/dev/null \
  || fail "checksum basename guard is missing"

printf 'release target matrix ok: %s\n' "$workflow"
