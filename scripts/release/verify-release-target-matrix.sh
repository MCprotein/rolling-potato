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

  awk \
    -v os="$os" \
    -v target="$target" \
    -v binary="$binary" \
    -v archive="$archive" '
      $0 == "          - os: " os {
        active = 1
        found_os = 1
        next
      }
      active && /^          - os: / { active = 0 }
      active && $0 == "            target: " target { found_target = 1 }
      active && $0 == "            binary: " binary { found_binary = 1 }
      active && $0 == "            archive: " archive { found_archive = 1 }
      END { exit !(found_os && found_target && found_binary && found_archive) }
    ' "$workflow" || fail "missing matrix tuple: $os/$target/$binary/$archive"
}

expect_entry "macos-26" "aarch64-apple-darwin" "rpotato" "tar.gz"
expect_entry "macos-26-intel" "x86_64-apple-darwin" "rpotato" "tar.gz"
expect_entry "ubuntu-24.04" "x86_64-unknown-linux-gnu" "rpotato" "tar.gz"
expect_entry "ubuntu-24.04-arm" "aarch64-unknown-linux-gnu" "rpotato" "tar.gz"
expect_entry "windows-2025" "x86_64-pc-windows-msvc" "rpotato.exe" "zip"

matrix_count="$(awk '/^          - os: / { count++ } END { print count + 0 }' "$workflow")"
[ "$matrix_count" -eq 5 ] || fail "expected 5 release matrix entries, found $matrix_count"

grep -F "name: release test gate" "$workflow" >/dev/null \
  || fail "release test gate job is missing"
grep -F "cargo test --locked" "$workflow" >/dev/null \
  || fail "release test gate must run cargo test --locked"
grep -F "      - test" "$workflow" >/dev/null \
  || fail "build job must depend on the release test gate"
grep -F "run: cargo test --locked -- --test-threads=1" "$workflow" >/dev/null \
  || fail "release test gate must run the complete locked test suite"
grep -F "name: Test native Windows backend lifecycle" "$workflow" >/dev/null \
  || fail "native Windows backend lifecycle test step is missing"
grep -F "if: matrix.target == 'x86_64-pc-windows-msvc'" "$workflow" >/dev/null \
  || fail "native lifecycle test must be scoped to the Windows target"
grep -F 'cargo test --locked --target ${{ matrix.target }} --bin rpotato backend_stream::tests:: -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows streaming transport tests are missing"
grep -F 'cargo test --locked --target ${{ matrix.target }} --bin rpotato generation_ -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows generation lifecycle tests are missing"
grep -F 'cargo test --locked --target ${{ matrix.target }} --test backend_lifecycle -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows backend process lifecycle test is missing"

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
