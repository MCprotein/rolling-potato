#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'package-manager workflow contract error: %s\n' "$1" >&2
  exit 1
}

workflow=".github/workflows/package-manager-distribution.yml"
release_workflow=".github/workflows/release-binaries.yml"
preflight="scripts/ci/verify-pr-candidate-preflight.sh"
validator="scripts/release/validate-package-manager-workflow-inputs.sh"
release_validator="scripts/release/verify-published-stable-release.sh"
resolver="scripts/release/resolve-previous-stable-tag.sh"
repo_root="$(pwd)"
checkout_pin='actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0'
upload_pin='actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1'
download_pin='actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1'

for file in \
  "$workflow" "$release_workflow" "$preflight" "$validator" \
  "$release_validator" "$resolver"; do
  [ -f "$file" ] || fail "required file is missing: $file"
done

job_block() {
  local workflow_path="$1"
  local job="$2"
  awk -v job="$job" '
    $0 == "  " job ":" { active = 1 }
    active && $0 ~ /^  [A-Za-z0-9_-]+:$/ && $0 != "  " job ":" { exit }
    active { print }
  ' "$workflow_path"
}

require_line() {
  local body="$1"
  local line="$2"
  grep -F -- "$line" <<<"$body" >/dev/null \
    || fail "missing contract line: $line"
}

expect_failure() {
  local case_name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    fail "negative fixture unexpectedly passed: $case_name"
  fi
  printf 'package-manager workflow fixture passed: %s\n' "$case_name"
}

workflow_body="$(cat "$workflow")"
release_body="$(cat "$release_workflow")"
prepare="$(job_block "$workflow" package-manager-prepare)"
homebrew="$(job_block "$workflow" homebrew-lifecycle)"
scoop="$(job_block "$workflow" scoop-lifecycle)"
winget="$(job_block "$workflow" winget-lifecycle)"
cleanup="$(job_block "$workflow" cleanup-release-branch)"
preserve="$(job_block "$workflow" package-manager-failure-preserves-branch)"
qualification_failure="$(job_block "$workflow" qualification-failure-summary)"
release_published="$(job_block "$release_workflow" published-assets-verify)"
release_package="$(job_block "$release_workflow" package-manager-distribution)"

require_line "$workflow_body" '  workflow_call:'
require_line "$workflow_body" '  workflow_dispatch:'
require_line "$workflow_body" '          - qualification'
require_line "$workflow_body" '          - recovery'
require_line "$workflow_body" '  contents: write'
require_line "$workflow_body" '  cancel-in-progress: false'
require_line "$workflow_body" '      CURRENT_TAG: ${{ inputs.current_tag }}'
require_line "$workflow_body" '      REQUESTED_PREVIOUS_TAG: ${{ inputs.previous_tag }}'

require_line "$prepare" "uses: $checkout_pin"
require_line "$prepare" "uses: $download_pin"
require_line "$prepare" "uses: $upload_pin"
require_line "$prepare" "scripts/release/validate-package-manager-workflow-inputs.sh"
require_line "$prepare" 'scripts/release/resolve-previous-stable-tag.sh "$CURRENT_TAG"'
require_line "$prepare" 'for release_tag in "$CURRENT_TAG" "$previous_tag"; do'
require_line "$prepare" '--json tagName,isDraft,isPrerelease,publishedAt'
require_line "$prepare" 'scripts/release/verify-published-stable-release.sh "$release_tag"'
require_line "$prepare" "if: inputs.mode == 'release'"
require_line "$prepare" "if: inputs.mode != 'release'"
require_line "$prepare" 'gh release download "$CURRENT_TAG" --repo "$GITHUB_REPOSITORY"'
require_line "$prepare" 'gh release download "$PREVIOUS_TAG" --repo "$GITHUB_REPOSITORY"'
require_line "$prepare" '"$CURRENT_TAG" dist/assets/current'
require_line "$prepare" '"$PREVIOUS_TAG" dist/assets/previous'
require_line "$prepare" 'dist/manifests/current'
require_line "$prepare" 'dist/manifests/previous'
require_line "$prepare" 'name: package-manager-current-${{ inputs.current_tag }}'
require_line "$prepare" 'name: package-manager-lifecycle-${{ inputs.current_tag }}'
[ "$(grep -Fc 'gh release download "$PREVIOUS_TAG"' <<<"$prepare")" -eq 1 ] \
  || fail "previous published assets must be downloaded exactly once"
[ "$(grep -Fc '"$PREVIOUS_TAG" dist/assets/previous' <<<"$prepare")" -eq 1 ] \
  || fail "previous exact assets must be verified exactly once"

for runner in macos-26 macos-26-intel ubuntu-24.04-arm ubuntu-24.04; do
  require_line "$homebrew" "            runner: $runner"
done
[ "$(grep -c '^          - label:' <<<"$homebrew")" -eq 4 ] \
  || fail "Homebrew matrix must contain exactly four native lanes"
require_line "$homebrew" 'uses: Homebrew/actions/setup-homebrew@df4b09108a1de9d6f995fe68f302b3f68bd6d2ef'
require_line "$homebrew" 'brew install rpotato/ci/rpotato'
require_line "$homebrew" 'brew update'
require_line "$homebrew" 'brew upgrade rpotato'
require_line "$homebrew" 'brew uninstall --force rpotato'

require_line "$scoop" '    runs-on: windows-2025'
require_line "$scoop" 'SCOOP_COMMIT: b588a06e41d920d2123ec70aee682bae14935939'
require_line "$scoop" 'New-Item -ItemType Directory -Force -Path (Join-Path $env:SCOOP "shims") | Out-Null'
require_line "$scoop" 'New-Item -ItemType Directory -Force -Path (Join-Path $env:SCOOP "buckets") | Out-Null'
require_line "$scoop" 'Test-Json -SchemaFile $schema'
require_line "$scoop" 'Invoke-Scoop install $currentManifest --no-update-scoop'
require_line "$scoop" "\$remoteUri = \"file:///\$(\$remote -replace '\\\\', '/')\""
require_line "$scoop" 'Invoke-Scoop bucket add rpotato $remoteUri'
require_line "$scoop" 'Invoke-Scoop install rpotato/rpotato --no-update-scoop'
require_line "$scoop" 'Invoke-Scoop update rpotato'
require_line "$scoop" 'Invoke-Scoop uninstall rpotato'

require_line "$winget" '    runs-on: windows-2025'
require_line "$winget" 'WINGET_VERSION: v1.29.280'
require_line "$winget" 'WINGET_BUNDLE_SHA256: 0809fa9f52e395d6e7de692331dce847ac991952675116bb4d8aae2ddcc20946'
require_line "$winget" 'WINGET_DEPENDENCIES_SHA256: 3bbfcaa5cb011c48fac48d896d64a5c7c6898859a9f3d01555c8cd000f4e2962'
require_line "$winget" 'Invoke-Winget settings --enable LocalManifestFiles'
require_line "$winget" 'Invoke-Winget validate --manifest $currentManifest'
require_line "$winget" 'Invoke-Winget install --manifest $currentManifest'
require_line "$winget" 'Invoke-Winget upgrade --manifest $currentManifest'
require_line "$winget" '$listed = (& winget list --id MCprotein.rpotato --exact --accept-source-agreements --disable-interactivity | Out-String)'
require_line "$winget" '$preexisting = (& winget list --id MCprotein.rpotato --exact --accept-source-agreements --disable-interactivity | Out-String)'
require_line "$winget" 'Invoke-Winget uninstall --manifest $currentManifest --accept-source-agreements --disable-interactivity'
require_line "$winget" '& winget uninstall --manifest $currentManifest --accept-source-agreements --disable-interactivity | Out-Null'
require_line "$winget" '& winget uninstall --manifest $previousManifest --accept-source-agreements --disable-interactivity | Out-Null'
require_line "$winget" '& winget settings --disable LocalManifestFiles'
[ "$(grep -Fc 'winget list --id MCprotein.rpotato --exact --accept-source-agreements --disable-interactivity' <<<"$winget")" -eq 2 ] \
  || fail "both winget list probes must accept source agreements"
[ "$(grep -Fc 'Invoke-Winget uninstall --manifest $currentManifest --accept-source-agreements --disable-interactivity' <<<"$winget")" -eq 2 ] \
  || fail "both winget lifecycle uninstalls must use the local current manifest"

if grep -E '(^|[[:space:]])cargo (test|check|build)|gh release upload|gh release create|git tag ' \
  "$workflow" >/dev/null; then
  fail "package-manager workflow must not build Cargo artifacts or mutate releases/tags"
fi
if grep -E 'MCprotein/(homebrew-rpotato|scoop-rpotato)|microsoft/winget-pkgs' \
  "$workflow" >/dev/null; then
  fail "package-manager workflow must not publish external channel state"
fi

require_line "$cleanup" "always() && (inputs.mode == 'release' || inputs.mode == 'recovery') &&"
for need in package-manager-prepare homebrew-lifecycle scoop-lifecycle winget-lifecycle; do
  require_line "$cleanup" "      - $need"
  require_line "$preserve" "      - $need"
done
require_line "$cleanup" '      RPOTATO_RELEASE_BRANCH: release/${{ inputs.current_tag }}'
require_line "$cleanup" '      RPOTATO_DELETE_RELEASE_BRANCH: 1'
require_line "$cleanup" '          ref: ${{ inputs.current_tag }}'
require_line "$cleanup" 'git ls-remote --exit-code --heads origin "release/$RELEASE_TAG"'
require_line "$preserve" '      RELEASE_BRANCH: release/${{ inputs.current_tag }}'
require_line "$preserve" 'scripts/release/report-release-failure.sh'
require_line "$qualification_failure" "always() && inputs.mode == 'qualification' &&"
require_line "$qualification_failure" "printf -- '- cleanup: \`forbidden\`\\n'"

delete_count="$(
  cat "$release_workflow" "$workflow" \
    | awk '/RPOTATO_DELETE_RELEASE_BRANCH:/ && $0 !~ /"0"/ { count++ } END { print count + 0 }'
)"
[ "$delete_count" -eq 1 ] || fail "normal/recovery release path must have one cleanup owner"
if grep -F 'RPOTATO_DELETE_RELEASE_BRANCH: 1' "$release_workflow" >/dev/null; then
  fail "release binary workflow must not own branch cleanup"
fi

require_line "$release_published" 'name: rpotato-${{ env.RELEASE_TAG }}-published-assets'
require_line "$release_published" 'path: dist/published'
require_line "$release_package" '      - published-assets-verify'
require_line "$release_package" 'uses: ./.github/workflows/package-manager-distribution.yml'
require_line "$release_package" '      mode: release'
require_line "$release_package" '      current_tag: ${{ github.event.release.tag_name }}'
require_line "$release_package" '      current_assets_artifact: rpotato-${{ github.event.release.tag_name }}-published-assets'
require_line "$(cat "$preflight")" 'bash scripts/release/test-package-manager-manifests.sh'
require_line "$(cat "$preflight")" 'bash scripts/release/test-package-manager-workflow-contract.sh'

"$validator" release v0.40.0 "" >/dev/null
"$validator" qualification v0.39.0 v0.38.0 >/dev/null
"$validator" recovery v0.40.0 "" >/dev/null
expect_failure invalid-mode "$validator" unknown v0.40.0 ""
expect_failure invalid-current "$validator" recovery v0.40 ""
expect_failure release-predecessor "$validator" release v0.40.0 v0.39.0
expect_failure recovery-predecessor "$validator" recovery v0.40.0 v0.39.0
expect_failure qualification-missing "$validator" qualification v0.39.0 ""
expect_failure qualification-reversed "$validator" qualification v0.38.0 v0.39.0

run_release_fixture() {
  local tag="$1"
  local json="$2"
  printf '%s\n' "$json" | "$release_validator" "$tag"
}

published_release='{"tagName":"v0.40.0","isDraft":false,"isPrerelease":false,"publishedAt":"2026-07-18T00:00:00Z"}'
run_release_fixture v0.40.0 "$published_release" >/dev/null
expect_failure published-release-draft \
  run_release_fixture v0.40.0 \
  '{"tagName":"v0.40.0","isDraft":true,"isPrerelease":false,"publishedAt":"2026-07-18T00:00:00Z"}'
expect_failure published-release-prerelease \
  run_release_fixture v0.40.0 \
  '{"tagName":"v0.40.0","isDraft":false,"isPrerelease":true,"publishedAt":"2026-07-18T00:00:00Z"}'
expect_failure published-release-wrong-tag \
  run_release_fixture v0.40.0 \
  '{"tagName":"v0.39.0","isDraft":false,"isPrerelease":false,"publishedAt":"2026-07-18T00:00:00Z"}'
expect_failure published-release-missing-date \
  run_release_fixture v0.40.0 \
  '{"tagName":"v0.40.0","isDraft":false,"isPrerelease":false,"publishedAt":null}'
expect_failure published-release-malformed-json \
  run_release_fixture v0.40.0 '{"tagName":'

resolver_fixture="$(mktemp -d)"
trap 'rm -rf "$resolver_fixture"' EXIT
git init --initial-branch=main --quiet "$resolver_fixture/repo"
git -C "$resolver_fixture/repo" config user.name package-manager-contract
git -C "$resolver_fixture/repo" config user.email package-manager-contract@example.invalid
printf 'v0.37.0\n' >"$resolver_fixture/repo/version"
git -C "$resolver_fixture/repo" add version
git -C "$resolver_fixture/repo" commit --quiet -m 'test: v0.37.0'
git -C "$resolver_fixture/repo" tag v0.37.0
printf 'v0.38.0\n' >"$resolver_fixture/repo/version"
git -C "$resolver_fixture/repo" commit --quiet -am 'test: v0.38.0'
git -C "$resolver_fixture/repo" tag v0.38.0
ancestor_commit="$(git -C "$resolver_fixture/repo" rev-parse HEAD)"
printf 'v0.39.0\n' >"$resolver_fixture/repo/version"
git -C "$resolver_fixture/repo" commit --quiet -am 'test: v0.39.0'
git -C "$resolver_fixture/repo" tag v0.39.0
git -C "$resolver_fixture/repo" tag v0.40.0
git -C "$resolver_fixture/repo" tag v0.38.5-rc.1
git -C "$resolver_fixture/repo" checkout --quiet --detach "$ancestor_commit"
printf 'divergent\n' >"$resolver_fixture/repo/divergent"
git -C "$resolver_fixture/repo" add divergent
git -C "$resolver_fixture/repo" commit --quiet -m 'test: divergent'
git -C "$resolver_fixture/repo" tag v0.38.9
git -C "$resolver_fixture/repo" checkout --quiet main
run_fixture_resolver() {
  (
    cd "$resolver_fixture/repo"
    "$repo_root/$resolver" "$@"
  )
}
resolved="$(run_fixture_resolver v0.39.0)"
[ "$resolved" = "v0.38.0" ] \
  || fail "resolver selected $resolved instead of greatest ancestral v0.38.0"
expect_failure resolver-missing-current \
  run_fixture_resolver v0.39.1
expect_failure resolver-prerelease-current \
  run_fixture_resolver v0.38.5-rc.1

report_fixture="$resolver_fixture/report"
report_remote="$report_fixture/remote.git"
report_seed="$report_fixture/seed"
report_stderr="$report_fixture/stderr"
git init --bare --quiet "$report_remote"
git init --initial-branch=main --quiet "$report_seed"
git -C "$report_seed" config user.name package-manager-contract
git -C "$report_seed" config user.email package-manager-contract@example.invalid
printf 'release branch\n' >"$report_seed/sentinel"
git -C "$report_seed" add sentinel
git -C "$report_seed" commit --quiet -m 'test: package-manager release branch'
git -C "$report_seed" branch release/v0.40.0
git -C "$report_seed" remote add fixture "$report_remote"
git -C "$report_seed" push --quiet fixture release/v0.40.0
scripts/release/report-release-failure.sh \
  homebrew-lifecycle:failure \
  'prepare=success,homebrew=failure,scoop=success,winget=success' \
  release/v0.40.0 "$report_remote" 2>"$report_stderr"
grep -F -- '- cause: homebrew-lifecycle:failure' "$report_stderr" >/dev/null \
  || fail "package-manager failure cause was not reported"
grep -F -- '- upstream: prepare=success,homebrew=failure,scoop=success,winget=success' \
  "$report_stderr" >/dev/null \
  || fail "package-manager upstream results were not reported"

ruby -e 'require "yaml"; YAML.load_file(ARGV.fetch(0))' "$workflow"

printf 'package-manager workflow contract ok: prepare-once six-lanes qualification recovery cleanup-owner\n'
