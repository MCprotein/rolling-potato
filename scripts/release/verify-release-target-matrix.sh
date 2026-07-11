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

grep -F "name: release test gate" "$workflow" >/dev/null \
  || fail "release test gate job is missing"
grep -F "cargo test --locked" "$workflow" >/dev/null \
  || fail "release test gate must run cargo test --locked"
grep -F "      - test" "$workflow" >/dev/null \
  || fail "build job must depend on the release test gate"
test_count="$(grep -F "cargo test --locked" "$workflow" | wc -l | tr -d ' ')"
if [ "$test_count" != "1" ]; then
  fail "cargo test --locked must run once in the release test gate, found $test_count"
fi

grep -F "name: Package tar.gz archive" "$workflow" >/dev/null \
  || fail "tar.gz packaging step must stay OS-neutral"
grep -F "name: Package Windows archive" "$workflow" >/dev/null \
  || fail "Windows zip packaging step is missing"
grep -F "scripts/release/verify-checksum-basenames.sh" "$workflow" >/dev/null \
  || fail "checksum basename guard is missing"
grep -F '$checksumLine = "$hash  $env:ASSET_BASE.zip`n"' "$workflow" >/dev/null \
  || fail "Windows checksum writer must terminate the line with explicit LF"
grep -F '[System.IO.File]::WriteAllText("$archive.sha256", $checksumLine, [System.Text.Encoding]::ASCII)' "$workflow" >/dev/null \
  || fail "Windows checksum writer must use BOM-free ASCII WriteAllText"

fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT
lf_fixture="$fixture_dir/lf.sha256"
crlf_fixture="$fixture_dir/crlf.sha256"
bom_fixture="$fixture_dir/bom.sha256"
checksum="0000000000000000000000000000000000000000000000000000000000000000"
printf '%s  artifact.zip\n' "$checksum" > "$lf_fixture"
printf '%s  artifact.zip\r\n' "$checksum" > "$crlf_fixture"
printf '\357\273\277%s  artifact.zip\n' "$checksum" > "$bom_fixture"
scripts/release/verify-checksum-basenames.sh "$lf_fixture"
if scripts/release/verify-checksum-basenames.sh "$crlf_fixture" >/dev/null 2>&1; then
  fail "checksum guard must reject CRLF input"
fi
if scripts/release/verify-checksum-basenames.sh "$bom_fixture" >/dev/null 2>&1; then
  fail "checksum guard must reject a UTF-8 BOM"
fi

printf 'release target matrix ok: %s\n' "$workflow"
