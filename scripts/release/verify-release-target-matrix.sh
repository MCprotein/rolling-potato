#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release target matrix error: %s\n' "$1" >&2
  exit 1
}

workflow=".github/workflows/release-binaries.yml"
manifest="config/release-targets.tsv"
runtime_owner="src/runtime_core/update.rs"

if [ ! -f "$workflow" ]; then
  fail "workflow file was not found: $workflow"
fi
if [ ! -f "$manifest" ]; then
  fail "release target manifest was not found: $manifest"
fi
if [ -e ".github/workflows/package-manager-distribution.yml" ]; then
  fail "external package-manager workflow must not exist"
fi
if [ -d "packaging/package-managers" ]; then
  fail "external package-manager manifests must not exist"
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

manifest_entries="$(awk -F '\t' '
  /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
  NF != 6 { exit 2 }
  { for (field = 1; field <= 6; field++) if ($field == "") exit 2; print }
' "$manifest")" || fail "release target manifest must contain six non-empty tab-separated fields"
[ -n "$manifest_entries" ] || fail "release target manifest is empty"
duplicate_platform="$(printf '%s\n' "$manifest_entries" | awk -F '\t' '
  { key = $1 "/" $2; if (seen[key]++) { print key; exit } }
')"
[ -z "$duplicate_platform" ] || fail "duplicate manifest platform: $duplicate_platform"
duplicate_target="$(printf '%s\n' "$manifest_entries" | awk -F '\t' '
  { if (seen[$3]++) { print $3; exit } }
')"
[ -z "$duplicate_target" ] || fail "duplicate manifest target: $duplicate_target"

while IFS=$'\t' read -r os arch target binary archive runner; do
  expect_entry "$runner" "$target" "$binary" "$archive"
done <<<"$manifest_entries"

matrix_count="$(awk '/^          - os: / { count++ } END { print count + 0 }' "$workflow")"
manifest_count="$(printf '%s\n' "$manifest_entries" | awk 'NF { count++ } END { print count + 0 }')"
[ "$matrix_count" -eq "$manifest_count" ] \
  || fail "workflow matrix count $matrix_count does not match manifest count $manifest_count"
grep -F 'include_str!("../../config/release-targets.tsv")' "$runtime_owner" >/dev/null \
  || fail "Rust updater must compile the canonical release target manifest"
grep -F 'config/release-targets.tsv' scripts/release/verify-release-assets.sh >/dev/null \
  || fail "release asset verifier must use the canonical release target manifest"

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
durable_proofs="scripts/release/verify-durable-runtime-proofs.sh"
if grep -E -- '--bin rpotato (observability|patch|runtime|ledger|state|transition)::tests::' "$durable_proofs" >/dev/null; then
  fail "durable proof entrypoint contains a stale pre-refactor selector"
fi
grep -F "test result: ok. 1 passed;" "$durable_proofs" >/dev/null \
  || fail "durable proof entrypoint must reject zero- and multiple-match selectors"
grep -F "name: Test native Windows backend lifecycle" "$workflow" >/dev/null \
  || fail "native Windows backend lifecycle test step is missing"
grep -F "if: matrix.target == 'x86_64-pc-windows-msvc'" "$workflow" >/dev/null \
  || fail "native lifecycle test must be scoped to the Windows target"
grep -F 'cargo test --locked --target ${{ matrix.target }} --bin rpotato adapters::llama_cpp::stream::tests:: -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows streaming transport tests are missing"
grep -F 'cargo test --locked --target ${{ matrix.target }} --bin rpotato app::inference_adapter::backend::tests:: -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows generation lifecycle tests are missing"
grep -F 'cargo test --locked --target ${{ matrix.target }} --test inference backend_lifecycle::native_backend_cancel_and_stop_lifecycle -- --test-threads=1' "$workflow" >/dev/null \
  || fail "native Windows backend process lifecycle test is missing"
grep -F 'cargo test --locked --target ${{ matrix.target }} --test surfaces native_terminal::entry_quit -- --exact --test-threads=1' "$workflow" >/dev/null \
  || fail "native terminal entry_quit test is missing from the build matrix"
grep -F 'cargo test --locked --target ${{ matrix.target }} --test surfaces native_terminal::full_adapter -- --exact --test-threads=1' "$workflow" >/dev/null \
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
if grep -Ei 'homebrew|scoop|winget|package-manager' "$workflow" >/dev/null; then
  fail "release workflow must remain GitHub-Releases-only"
fi
grep -F 'name: cleanup verified release branch' "$workflow" >/dev/null \
  || fail "verified binary release branch cleanup job is missing"
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

printf 'release target matrix ok: github-release-binaries=%s\n' "$manifest_count"
