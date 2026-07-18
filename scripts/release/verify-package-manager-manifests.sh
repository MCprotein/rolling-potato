#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'package-manager verification error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] \
  || fail "usage: scripts/release/verify-package-manager-manifests.sh TAG CHECKSUM_FILE OUT"

tag="$1"
checksum_file="$2"
output_root="$3"
[[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail "tag must be a strict stable semver: $tag"
version="${tag#v}"
[ -f "$checksum_file" ] && [ ! -L "$checksum_file" ] \
  || fail "checksum input must be a regular non-symlink file"
[ -d "$output_root" ] && [ ! -L "$output_root" ] \
  || fail "output root must be a regular directory"
if find "$output_root" -type l -print -quit | grep -q .; then
  fail "generated output must not contain symlinks"
fi

mac_arm_archive="rpotato-${tag}-aarch64-apple-darwin.tar.gz"
mac_x64_archive="rpotato-${tag}-x86_64-apple-darwin.tar.gz"
linux_arm_archive="rpotato-${tag}-aarch64-unknown-linux-gnu.tar.gz"
linux_x64_archive="rpotato-${tag}-x86_64-unknown-linux-gnu.tar.gz"
windows_x64_archive="rpotato-${tag}-x86_64-pc-windows-msvc.zip"

checksum_for() {
  local name="$1"
  local matches
  matches="$(awk -v name="$name" '$2 == name { print $1 }' "$checksum_file")"
  [ "$(printf '%s\n' "$matches" | awk 'NF { count++ } END { print count + 0 }')" -eq 1 ] \
    || fail "checksum must appear exactly once: $name"
  [[ "$matches" =~ ^[0-9a-f]{64}$ ]] \
    || fail "checksum must be lowercase SHA-256: $name"
  printf '%s' "$matches"
}

mac_arm_hash="$(checksum_for "$mac_arm_archive")"
mac_x64_hash="$(checksum_for "$mac_x64_archive")"
linux_arm_hash="$(checksum_for "$linux_arm_archive")"
linux_x64_hash="$(checksum_for "$linux_x64_archive")"
windows_x64_hash="$(checksum_for "$windows_x64_archive")"
[ "$(awk 'NF { count++ } END { print count + 0 }' "$checksum_file")" -eq 5 ] \
  || fail "checksum input must contain exactly five entries"

formula="homebrew/Formula/rpotato.rb"
scoop="scoop/bucket/rpotato.json"
winget_root="winget/manifests/m/MCprotein/rpotato/$version"
winget_version="$winget_root/MCprotein.rpotato.yaml"
winget_locale="$winget_root/MCprotein.rpotato.locale.en-US.yaml"
winget_installer="$winget_root/MCprotein.rpotato.installer.yaml"
expected_files=(
  "$formula"
  "$scoop"
  "$winget_version"
  "$winget_locale"
  "$winget_installer"
)

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-package-manager-verify.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT
printf '%s\n' "${expected_files[@]}" | LC_ALL=C sort >"$tmp_root/expected-files.txt"
find "$output_root" -type f -print \
  | sed "s|^$output_root/||" \
  | LC_ALL=C sort >"$tmp_root/actual-files.txt"
cmp -s "$tmp_root/expected-files.txt" "$tmp_root/actual-files.txt" \
  || fail "generated file set differs from the exact five-file contract"

for relative in "${expected_files[@]}"; do
  path="$output_root/$relative"
  [ -f "$path" ] && [ ! -L "$path" ] || fail "missing regular file: $relative"
  prefix="$(od -An -tx1 -N3 "$path" | tr -d ' \n')"
  [ "$prefix" != "efbbbf" ] || fail "UTF-8 BOM is forbidden: $relative"
  if LC_ALL=C grep -q $'\r' "$path"; then
    fail "CR bytes are forbidden: $relative"
  fi
  [ "$(tail -c 1 "$path" | od -An -tx1 | tr -d ' \n')" = "0a" ] \
    || fail "generated files must end with LF: $relative"
  if grep -Eq '@@[A-Z0-9_]+@@' "$path"; then
    fail "unresolved template token: $relative"
  fi
done

base_url="https://github.com/MCprotein/rolling-potato/releases/download/$tag"
formula_path="$output_root/$formula"
grep -Fx 'class Rpotato < Formula' "$formula_path" >/dev/null \
  || fail "Homebrew formula class mismatch"
grep -Fx '  desc "Local coding agents for potato PCs."' "$formula_path" >/dev/null \
  || fail "Homebrew description mismatch"
grep -Fx '  homepage "https://github.com/MCprotein/rolling-potato"' "$formula_path" >/dev/null \
  || fail "Homebrew homepage mismatch"
grep -Fx "  version \"$version\"" "$formula_path" >/dev/null \
  || fail "Homebrew version mismatch"
grep -Fx '  license "Apache-2.0"' "$formula_path" >/dev/null \
  || fail "Homebrew license mismatch"
grep -Fx '    bin.install "rpotato"' "$formula_path" >/dev/null \
  || fail "Homebrew binary path mismatch"
grep -Fx "    assert_match \"package version: $version\", shell_output(\"#{bin}/rpotato doctor\")" "$formula_path" >/dev/null \
  || fail "Homebrew doctor test mismatch"
for pair in \
  "$base_url/$mac_arm_archive|$mac_arm_hash" \
  "$base_url/$mac_x64_archive|$mac_x64_hash" \
  "$base_url/$linux_arm_archive|$linux_arm_hash" \
  "$base_url/$linux_x64_archive|$linux_x64_hash"; do
  url="${pair%%|*}"
  hash="${pair#*|}"
  grep -F "url \"$url\"" "$formula_path" >/dev/null \
    || fail "Homebrew URL mismatch: $url"
  grep -F "sha256 \"$hash\"" "$formula_path" >/dev/null \
    || fail "Homebrew checksum mismatch: $url"
done
command -v ruby >/dev/null 2>&1 || fail "ruby is required for formula syntax validation"
ruby -c "$formula_path" >/dev/null \
  || fail "Homebrew formula is not valid Ruby"

scoop_path="$output_root/$scoop"
command -v jq >/dev/null 2>&1 || fail "jq is required for Scoop JSON validation"
jq -e \
  --arg version "$version" \
  --arg url "$base_url/$windows_x64_archive" \
  --arg hash "$windows_x64_hash" \
  '
    .version == $version
    and .description == "Local coding agents for potato PCs."
    and .homepage == "https://github.com/MCprotein/rolling-potato"
    and .license == "Apache-2.0"
    and .architecture["64bit"].url == $url
    and .architecture["64bit"].hash == $hash
    and .bin == "rpotato.exe"
    and .checkver.github == "https://github.com/MCprotein/rolling-potato"
    and .autoupdate.architecture["64bit"].url
      == "https://github.com/MCprotein/rolling-potato/releases/download/v$version/rpotato-v$version-x86_64-pc-windows-msvc.zip"
  ' "$scoop_path" >/dev/null \
  || fail "Scoop manifest fields mismatch"

version_path="$output_root/$winget_version"
locale_path="$output_root/$winget_locale"
installer_path="$output_root/$winget_installer"
for path in "$version_path" "$locale_path" "$installer_path"; do
  grep -Fx "PackageIdentifier: MCprotein.rpotato" "$path" >/dev/null \
    || fail "winget identifier mismatch: $path"
  grep -Fx "PackageVersion: $version" "$path" >/dev/null \
    || fail "winget version mismatch: $path"
  grep -Fx "ManifestVersion: 1.12.0" "$path" >/dev/null \
    || fail "winget schema version mismatch: $path"
done
grep -Fx "DefaultLocale: en-US" "$version_path" >/dev/null \
  || fail "winget default locale mismatch"
grep -Fx "ManifestType: version" "$version_path" >/dev/null \
  || fail "winget version manifest type mismatch"
grep -Fx "PackageLocale: en-US" "$locale_path" >/dev/null \
  || fail "winget locale mismatch"
grep -Fx "Publisher: MCprotein" "$locale_path" >/dev/null \
  || fail "winget publisher mismatch"
grep -Fx "PackageName: rpotato" "$locale_path" >/dev/null \
  || fail "winget package name mismatch"
grep -Fx "ManifestType: defaultLocale" "$locale_path" >/dev/null \
  || fail "winget locale manifest type mismatch"
grep -Fx "InstallerType: zip" "$installer_path" >/dev/null \
  || fail "winget installer type mismatch"
grep -Fx "NestedInstallerType: portable" "$installer_path" >/dev/null \
  || fail "winget nested installer type mismatch"
grep -Fx "  - Architecture: x64" "$installer_path" >/dev/null \
  || fail "winget architecture mismatch"
grep -Fx "    InstallerUrl: $base_url/$windows_x64_archive" "$installer_path" >/dev/null \
  || fail "winget URL mismatch"
grep -Fx "    InstallerSha256: $windows_x64_hash" "$installer_path" >/dev/null \
  || fail "winget checksum mismatch"
grep -Fx "      - RelativeFilePath: rpotato.exe" "$installer_path" >/dev/null \
  || fail "winget nested executable path mismatch"
grep -Fx "        PortableCommandAlias: rpotato" "$installer_path" >/dev/null \
  || fail "winget command alias mismatch"
grep -Fx "ManifestType: installer" "$installer_path" >/dev/null \
  || fail "winget installer manifest type mismatch"

printf 'package-manager manifests ok: tag=%s output=%s files=5\n' \
  "$tag" "$output_root"
