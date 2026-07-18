#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'package-manager prerequisite error: %s\n' "$1" >&2
  exit 1
}

workflow=".github/workflows/package-manager-distribution.yml"
[ -f "$workflow" ] || fail "workflow is missing: $workflow"

homebrew_pin='Homebrew/actions/setup-homebrew@df4b09108a1de9d6f995fe68f302b3f68bd6d2ef'
scoop_pin='b588a06e41d920d2123ec70aee682bae14935939'
winget_version='v1.29.280'
winget_bundle_hash='0809fa9f52e395d6e7de692331dce847ac991952675116bb4d8aae2ddcc20946'
winget_dependencies_hash='3bbfcaa5cb011c48fac48d896d64a5c7c6898859a9f3d01555c8cd000f4e2962'

for value in \
  "$homebrew_pin" \
  "SCOOP_COMMIT: $scoop_pin" \
  "WINGET_VERSION: $winget_version" \
  "WINGET_BUNDLE_SHA256: $winget_bundle_hash" \
  "WINGET_DEPENDENCIES_SHA256: $winget_dependencies_hash" \
  'Invoke-Winget settings --enable LocalManifestFiles' \
  '& winget settings --disable LocalManifestFiles'; do
  grep -F -- "$value" "$workflow" >/dev/null \
    || fail "workflow is missing immutable prerequisite: $value"
done

if grep -Ein \
  '(curl|wget)[^|]*(\||[[:space:]])[[:space:]]*(sh|bash)|Invoke-(WebRequest|RestMethod)[^|]*\|[[:space:]]*Invoke-Expression|(^|[^A-Za-z])(irm|iwr)[^|]*\|[[:space:]]*(iex|Invoke-Expression)' \
  "$workflow" >/dev/null; then
  fail "unpinned remote bootstrap execution is forbidden"
fi

while IFS= read -r action_ref; do
  case "$action_ref" in
    ./*) continue ;;
  esac
  [[ "$action_ref" =~ @[0-9a-f]{40}$ ]] \
    || fail "remote action must use a full immutable SHA: $action_ref"
done < <(sed -n 's/^[[:space:]]*uses:[[:space:]]*\([^[:space:]#]*\).*$/\1/p' "$workflow")

grep -F 'git clone https://github.com/ScoopInstaller/Scoop.git' "$workflow" >/dev/null \
  || fail "Scoop must be cloned from the official repository"
grep -F 'git -C "$env:SCOOP\apps\scoop\current" checkout --detach $env:SCOOP_COMMIT' \
  "$workflow" >/dev/null \
  || fail "Scoop clone must be detached at the pinned commit"
grep -F 'schema.json' "$workflow" >/dev/null \
  || fail "the pinned Scoop schema must be used"
grep -F 'https://github.com/microsoft/winget-cli/releases/download/$env:WINGET_VERSION' \
  "$workflow" >/dev/null \
  || fail "winget assets must come from the pinned official release"

printf 'package-manager prerequisites ok: homebrew=%s scoop=%s winget=%s\n' \
  "${homebrew_pin##*@}" "$scoop_pin" "$winget_version"
