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
grep -F "run: scripts/release/verify-durable-runtime-proofs.sh" "$workflow" >/dev/null \
  || fail "release workflow must invoke the stable durable runtime proof entrypoint"
if grep -F "cargo test --locked --bin rpotato observability::tests::" "$workflow" >/dev/null; then
  fail "release workflow must not depend on module-qualified durable proof names"
fi
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
grep -F 'cargo test --locked --target ${{ matrix.target }} --test platform native_terminal::entry_quit -- --exact --test-threads=1' "$workflow" >/dev/null \
  || fail "native terminal entry_quit test is missing from the build matrix"
grep -F 'cargo test --locked --target ${{ matrix.target }} --test platform native_terminal::full_adapter -- --exact --test-threads=1' "$workflow" >/dev/null \
  || fail "native terminal full_adapter test is missing from the build matrix"
grep -F 'bash scripts/release/test-verify-release-assets.sh' "$workflow" >/dev/null \
  || fail "release asset verifier fixture gate is missing"
grep -F 'bash scripts/release/test-release-workflow-contract.sh' "$workflow" >/dev/null \
  || fail "release workflow contract fixture gate is missing"
grep -F 'RPOTATO_TEST_TERMINAL_FAULT="invalid-release-smoke-value"' scripts/release/verify-release-binary-smoke.sh >/dev/null \
  || fail "release binary smoke must prove the debug-only terminal fault seam is ignored"

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
grep -F 'scripts/release/verify-release-assets.sh "$RELEASE_TAG" dist/release-assets' "$workflow" >/dev/null \
  || fail "aggregate checksum job must validate the exact release asset set"
grep -F 'name: verify published release assets' "$workflow" >/dev/null \
  || fail "published release asset verification job is missing"
grep -F 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY"' "$workflow" >/dev/null \
  || fail "published verification must download the complete uploaded asset set"
if grep -F 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --pattern' "$workflow" >/dev/null; then
  fail "published verification must not hide unexpected assets behind a download pattern"
fi
grep -F 'scripts/release/verify-release-assets.sh "$RELEASE_TAG" dist/published' "$workflow" >/dev/null \
  || fail "published assets must be downloaded and verified"
grep -F 'name: cleanup verified release branch' "$workflow" >/dev/null \
  || fail "release branch cleanup job is missing"
grep -F '      - published-assets-verify' "$workflow" >/dev/null \
  || fail "release branch cleanup must depend on published asset verification"
grep -F 'RPOTATO_DELETE_RELEASE_BRANCH: "0"' .github/workflows/release-policy.yml >/dev/null \
  || fail "release policy workflow must not delete the branch before asset verification"

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
