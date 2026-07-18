#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
generator="$script_dir/generate-package-manager-manifests.sh"
current_generator="$script_dir/generate-current-package-manager-manifests.sh"
verifier="$script_dir/verify-package-manager-manifests.sh"
fixture_root="$repo_root/packaging/package-managers/fixtures"
checksum_fixture="$fixture_root/checksums-v0.40.0.txt"
expected="$fixture_root/expected"
root="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/rpotato-package-manager-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$root"' EXIT

expect_failure() {
  local case_name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    printf 'package-manager fixture unexpectedly passed: %s\n' "$case_name" >&2
    exit 1
  fi
  printf 'package-manager fixture passed: %s\n' "$case_name"
}

generated="$root/generated"
"$generator" v0.40.0 "$checksum_fixture" "$generated" >/dev/null
"$verifier" v0.40.0 "$checksum_fixture" "$generated" >/dev/null
diff -ru "$expected" "$generated"
printf 'package-manager fixture passed: exact-output\n'

formula_fixture="$generated/homebrew/Formula/rpotato.rb"
grep -Fx '  desc "Local coding agents for potato PCs"' "$formula_fixture" >/dev/null
if grep -Eq '^[[:space:]]+version "' "$formula_fixture"; then
  printf 'package-manager fixture error: Homebrew formula must infer version from URL\n' >&2
  exit 1
fi
printf 'package-manager fixture passed: homebrew-audit-shape\n'

winget_fixture_root="$generated/winget/manifests/m/MCprotein/rpotato/0.40.0"
grep -Fx '# yaml-language-server: $schema=https://aka.ms/winget-manifest.version.1.12.0.schema.json' \
  "$winget_fixture_root/MCprotein.rpotato.yaml" >/dev/null
grep -Fx '# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.1.12.0.schema.json' \
  "$winget_fixture_root/MCprotein.rpotato.locale.en-US.yaml" >/dev/null
grep -Fx '# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.1.12.0.schema.json' \
  "$winget_fixture_root/MCprotein.rpotato.installer.yaml" >/dev/null
printf 'package-manager fixture passed: winget-schema-headers\n'

second="$root/generated-second"
"$generator" v0.40.0 "$checksum_fixture" "$second" >/dev/null
diff -ru "$generated" "$second"
printf 'package-manager fixture passed: idempotent\n'

cargo_version="$(
  sed -n 's/^version = "\([^"]*\)"/\1/p' "$repo_root/Cargo.toml" | head -n 1
)"
current_tag="v$cargo_version"
current_checksums="$root/current-checksums.txt"
sed "s/v0\\.40\\.0/$current_tag/g" "$checksum_fixture" >"$current_checksums"
"$current_generator" "$current_tag" "$current_checksums" "$root/current" >/dev/null
printf 'package-manager fixture passed: current-wrapper\n'

mismatch_checksums="$root/mismatch-checksums.txt"
sed 's/v0\\.40\\.0/v9.9.9/g' "$checksum_fixture" >"$mismatch_checksums"
expect_failure current-wrapper-mismatch \
  "$current_generator" v9.9.9 "$mismatch_checksums" "$root/mismatch"

for invalid_tag in \
  0.40.0 v0.40 v01.2.3 v0.040.0 v0.40.0-rc.1 v0.40.0/escape; do
  expect_failure "invalid-tag/$invalid_tag" \
    "$generator" "$invalid_tag" "$checksum_fixture" "$root/invalid-tag"
done

missing="$root/missing.txt"
sed -n '1,4p' "$checksum_fixture" >"$missing"
expect_failure missing-asset "$generator" v0.40.0 "$missing" "$root/missing"

extra="$root/extra.txt"
cp "$checksum_fixture" "$extra"
printf '%064d  unexpected.zip\n' 0 >>"$extra"
expect_failure extra-asset "$generator" v0.40.0 "$extra" "$root/extra"

duplicate="$root/duplicate.txt"
cp "$checksum_fixture" "$duplicate"
sed -n '1p' "$checksum_fixture" >>"$duplicate"
expect_failure duplicate-asset "$generator" v0.40.0 "$duplicate" "$root/duplicate"

uppercase="$root/uppercase.txt"
tr 'a-f' 'A-F' <"$checksum_fixture" >"$uppercase"
expect_failure uppercase-hash "$generator" v0.40.0 "$uppercase" "$root/uppercase"

wrong_hash="$root/wrong-hash.txt"
sed '1s/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/' \
  "$checksum_fixture" >"$wrong_hash"
expect_failure wrong-hash "$generator" v0.40.0 "$wrong_hash" "$root/wrong-hash"

path_name="$root/path-name.txt"
awk 'NR == 1 { sub(/  rpotato/, "  nested/rpotato") } { print }' \
  "$checksum_fixture" >"$path_name"
expect_failure path-bearing-basename \
  "$generator" v0.40.0 "$path_name" "$root/path-name"

unsorted="$root/unsorted.txt"
awk '{ lines[NR] = $0 } END { for (idx = NR; idx > 0; idx--) print lines[idx] }' \
  "$checksum_fixture" >"$unsorted"
expect_failure unsorted-checksums \
  "$generator" v0.40.0 "$unsorted" "$root/unsorted"

crlf="$root/crlf.txt"
sed 's/$/\r/' "$checksum_fixture" >"$crlf"
expect_failure checksum-crlf "$generator" v0.40.0 "$crlf" "$root/crlf-output"

bom="$root/bom.txt"
printf '\357\273\277' >"$bom"
cat "$checksum_fixture" >>"$bom"
expect_failure checksum-bom "$generator" v0.40.0 "$bom" "$root/bom-output"

nonempty="$root/nonempty"
mkdir -p "$nonempty"
printf 'occupied\n' >"$nonempty/file"
expect_failure nonempty-output \
  "$generator" v0.40.0 "$checksum_fixture" "$nonempty"

mkdir -p "$root/symlink-target"
ln -s "$root/symlink-target" "$root/symlink-output"
expect_failure symlink-output \
  "$generator" v0.40.0 "$checksum_fixture" "$root/symlink-output"

mkdir -p "$root/symlink-parent-target"
ln -s "$root/symlink-parent-target" "$root/symlink-parent"
expect_failure symlink-output-ancestor \
  "$generator" v0.40.0 "$checksum_fixture" "$root/symlink-parent/child"

ln -s "$checksum_fixture" "$root/checksums-link.txt"
expect_failure symlink-input \
  "$generator" v0.40.0 "$root/checksums-link.txt" "$root/symlink-input"

mkdir -p "$root/traversal"
expect_failure output-traversal \
  "$generator" v0.40.0 "$checksum_fixture" "$root/traversal/../escape"

stale="$root/stale"
cp -R "$generated" "$stale"
sed 's/package version: 0.40.0/package version: 0.41.0/' \
  "$stale/homebrew/Formula/rpotato.rb" >"$root/stale-formula"
mv "$root/stale-formula" "$stale/homebrew/Formula/rpotato.rb"
expect_failure stale-version "$verifier" v0.40.0 "$checksum_fixture" "$stale"

redundant_version="$root/redundant-homebrew-version"
cp -R "$generated" "$redundant_version"
awk '
  { print }
  /^  homepage / { print "  version \"0.40.0\"" }
' "$redundant_version/homebrew/Formula/rpotato.rb" >"$root/redundant-formula"
mv "$root/redundant-formula" \
  "$redundant_version/homebrew/Formula/rpotato.rb"
expect_failure redundant-homebrew-version \
  "$verifier" v0.40.0 "$checksum_fixture" "$redundant_version"

bad_url="$root/bad-url"
cp -R "$generated" "$bad_url"
sed 's|github.com/MCprotein/rolling-potato|example.invalid/rolling-potato|' \
  "$bad_url/scoop/bucket/rpotato.json" >"$root/bad-scoop"
mv "$root/bad-scoop" "$bad_url/scoop/bucket/rpotato.json"
expect_failure wrong-url-origin "$verifier" v0.40.0 "$checksum_fixture" "$bad_url"

bad_hash="$root/bad-output-hash"
cp -R "$generated" "$bad_hash"
sed 's/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff/' \
  "$bad_hash/winget/manifests/m/MCprotein/rpotato/0.40.0/MCprotein.rpotato.installer.yaml" \
  >"$root/bad-installer"
mv "$root/bad-installer" \
  "$bad_hash/winget/manifests/m/MCprotein/rpotato/0.40.0/MCprotein.rpotato.installer.yaml"
expect_failure wrong-output-hash \
  "$verifier" v0.40.0 "$checksum_fixture" "$bad_hash"

missing_schema_header="$root/missing-schema-header"
cp -R "$generated" "$missing_schema_header"
sed '1d' \
  "$missing_schema_header/winget/manifests/m/MCprotein/rpotato/0.40.0/MCprotein.rpotato.yaml" \
  >"$root/winget-version-without-schema-header"
mv "$root/winget-version-without-schema-header" \
  "$missing_schema_header/winget/manifests/m/MCprotein/rpotato/0.40.0/MCprotein.rpotato.yaml"
expect_failure missing-winget-schema-header \
  "$verifier" v0.40.0 "$checksum_fixture" "$missing_schema_header"

unresolved="$root/unresolved"
cp -R "$generated" "$unresolved"
printf '@@UNRESOLVED@@\n' >>"$unresolved/homebrew/Formula/rpotato.rb"
expect_failure unresolved-token \
  "$verifier" v0.40.0 "$checksum_fixture" "$unresolved"

unexpected="$root/unexpected"
cp -R "$generated" "$unexpected"
printf 'unexpected\n' >"$unexpected/extra.txt"
expect_failure unexpected-output \
  "$verifier" v0.40.0 "$checksum_fixture" "$unexpected"

missing_template_root="$root/missing-template-repo"
mkdir -p "$missing_template_root/scripts/release" \
  "$missing_template_root/packaging/package-managers"
cp "$generator" "$missing_template_root/scripts/release/"
cp -R "$repo_root/packaging/package-managers/metadata.conf" \
  "$repo_root/packaging/package-managers/templates" \
  "$missing_template_root/packaging/package-managers/"
rm -f "$missing_template_root/packaging/package-managers/templates/winget-installer.yaml.in"
expect_failure missing-template \
  "$missing_template_root/scripts/release/generate-package-manager-manifests.sh" \
  v0.40.0 "$checksum_fixture" "$root/missing-template-output"

printf 'package-manager manifest fixtures ok\n'
