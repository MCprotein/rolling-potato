#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'package-manager generation error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] \
  || fail "usage: scripts/release/generate-package-manager-manifests.sh TAG CHECKSUM_FILE OUT"

tag="$1"
checksum_file="$2"
output_root="$3"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
package_root="$repo_root/packaging/package-managers"
metadata="$package_root/metadata.conf"
templates="$package_root/templates"

[[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail "tag must be a strict stable semver: $tag"
version="${tag#v}"

[ -f "$checksum_file" ] && [ ! -L "$checksum_file" ] \
  || fail "checksum input must be a regular non-symlink file: $checksum_file"
[ -f "$metadata" ] && [ ! -L "$metadata" ] \
  || fail "metadata must be a regular non-symlink file: $metadata"
case "/$output_root/" in
  */../* | */./*) fail "output path traversal is forbidden: $output_root" ;;
esac
if [ -L "$output_root" ]; then
  fail "output root must not be a symlink: $output_root"
fi
if [ -e "$output_root" ] && [ ! -d "$output_root" ]; then
  fail "output root must be a directory: $output_root"
fi
if [ -d "$output_root" ] \
  && [ -n "$(find "$output_root" -mindepth 1 -maxdepth 1 -print -quit)" ]; then
  fail "output root must be new or empty: $output_root"
fi

reject_text_encoding() {
  local path="$1"
  local prefix last
  prefix="$(od -An -tx1 -N3 "$path" | tr -d ' \n')"
  [ "$prefix" != "efbbbf" ] || fail "UTF-8 BOM is forbidden: $path"
  if LC_ALL=C grep -q $'\r' "$path"; then
    fail "CR bytes are forbidden: $path"
  fi
  last="$(tail -c 1 "$path" | od -An -tx1 | tr -d ' \n')"
  [ "$last" = "0a" ] || fail "file must end with LF: $path"
}

reject_text_encoding "$checksum_file"
reject_text_encoding "$metadata"

metadata_keys=(
  PACKAGE_NAME
  DESCRIPTION
  HOMEPAGE
  LICENSE
  REPOSITORY
  PUBLISHER
  HOMEBREW_FORMULA
  SCOOP_APP
  WINGET_ID
  WINGET_LOCALE
)

[ "$(wc -l <"$metadata" | tr -d ' ')" -eq "${#metadata_keys[@]}" ] \
  || fail "metadata must contain exactly ${#metadata_keys[@]} entries"
while IFS= read -r line; do
  key="${line%%=*}"
  value="${line#*=}"
  [ "$key" != "$line" ] && [ -n "$value" ] \
    || fail "metadata entries must be non-empty KEY=VALUE pairs"
  known=0
  for expected_key in "${metadata_keys[@]}"; do
    if [ "$key" = "$expected_key" ]; then
      known=1
      break
    fi
  done
  [ "$known" -eq 1 ] || fail "unknown metadata key: $key"
done <"$metadata"

metadata_value() {
  local key="$1"
  local count line
  count="$(grep -Fxc "$key=" "$metadata" || true)"
  if [ "$count" -ne 0 ]; then
    fail "metadata key has an empty value: $key"
  fi
  count="$(grep -c "^${key}=" "$metadata" || true)"
  [ "$count" -eq 1 ] || fail "metadata key must appear exactly once: $key"
  line="$(grep "^${key}=" "$metadata")"
  printf '%s' "${line#*=}"
}

package_name="$(metadata_value PACKAGE_NAME)"
description="$(metadata_value DESCRIPTION)"
homepage="$(metadata_value HOMEPAGE)"
license="$(metadata_value LICENSE)"
repository="$(metadata_value REPOSITORY)"
publisher="$(metadata_value PUBLISHER)"
homebrew_formula="$(metadata_value HOMEBREW_FORMULA)"
scoop_app="$(metadata_value SCOOP_APP)"
winget_id="$(metadata_value WINGET_ID)"
winget_locale="$(metadata_value WINGET_LOCALE)"

[ "$package_name" = "rpotato" ] || fail "unexpected package name: $package_name"
[ "$repository" = "MCprotein/rolling-potato" ] \
  || fail "unexpected repository: $repository"
[ "$homebrew_formula" = "rpotato" ] \
  || fail "unexpected Homebrew formula: $homebrew_formula"
[ "$scoop_app" = "rpotato" ] || fail "unexpected Scoop app: $scoop_app"
[ "$winget_id" = "MCprotein.rpotato" ] \
  || fail "unexpected winget identifier: $winget_id"

mac_arm_archive="rpotato-${tag}-aarch64-apple-darwin.tar.gz"
mac_x64_archive="rpotato-${tag}-x86_64-apple-darwin.tar.gz"
linux_arm_archive="rpotato-${tag}-aarch64-unknown-linux-gnu.tar.gz"
linux_x64_archive="rpotato-${tag}-x86_64-unknown-linux-gnu.tar.gz"
windows_x64_archive="rpotato-${tag}-x86_64-pc-windows-msvc.zip"
mac_arm_hash=""
mac_x64_hash=""
linux_arm_hash=""
linux_x64_hash=""
windows_x64_hash=""
checksum_count=0

while IFS= read -r line; do
  [[ "$line" =~ ^([0-9a-f]{64})\ \ ([^/\\]+)$ ]] \
    || fail "checksum lines must use lowercase SHA-256 and a basename"
  hash="${BASH_REMATCH[1]}"
  name="${BASH_REMATCH[2]}"
  case "$name" in
    "$mac_arm_archive")
      [ -z "$mac_arm_hash" ] || fail "duplicate checksum: $name"
      mac_arm_hash="$hash"
      ;;
    "$mac_x64_archive")
      [ -z "$mac_x64_hash" ] || fail "duplicate checksum: $name"
      mac_x64_hash="$hash"
      ;;
    "$linux_arm_archive")
      [ -z "$linux_arm_hash" ] || fail "duplicate checksum: $name"
      linux_arm_hash="$hash"
      ;;
    "$linux_x64_archive")
      [ -z "$linux_x64_hash" ] || fail "duplicate checksum: $name"
      linux_x64_hash="$hash"
      ;;
    "$windows_x64_archive")
      [ -z "$windows_x64_hash" ] || fail "duplicate checksum: $name"
      windows_x64_hash="$hash"
      ;;
    *) fail "unexpected checksum asset: $name" ;;
  esac
  checksum_count=$((checksum_count + 1))
done <"$checksum_file"

[ "$checksum_count" -eq 5 ] || fail "checksum input must contain exactly five entries"
for required_hash in \
  "$mac_arm_hash" "$mac_x64_hash" "$linux_arm_hash" "$linux_x64_hash" \
  "$windows_x64_hash"; do
  [ -n "$required_hash" ] || fail "checksum input is missing a required archive"
done

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-package-manager.XXXXXX")"
trap 'rm -rf "$tmp_root"' EXIT
LC_ALL=C sort -t ' ' -k3,3 "$checksum_file" >"$tmp_root/sorted-checksums.txt"
cmp -s "$checksum_file" "$tmp_root/sorted-checksums.txt" \
  || fail "checksum input must use canonical basename order"

base_url="https://github.com/${repository}/releases/download/${tag}"
tokens=(
  "@@PACKAGE_NAME@@"
  "@@DESCRIPTION@@"
  "@@HOMEPAGE@@"
  "@@LICENSE@@"
  "@@PUBLISHER@@"
  "@@WINGET_ID@@"
  "@@WINGET_LOCALE@@"
  "@@VERSION@@"
  "@@URL_MAC_ARM@@"
  "@@SHA_MAC_ARM@@"
  "@@URL_MAC_X64@@"
  "@@SHA_MAC_X64@@"
  "@@URL_LINUX_ARM@@"
  "@@SHA_LINUX_ARM@@"
  "@@URL_LINUX_X64@@"
  "@@SHA_LINUX_X64@@"
  "@@URL_WINDOWS_X64@@"
  "@@SHA_WINDOWS_X64@@"
)
values=(
  "$package_name"
  "$description"
  "$homepage"
  "$license"
  "$publisher"
  "$winget_id"
  "$winget_locale"
  "$version"
  "$base_url/$mac_arm_archive"
  "$mac_arm_hash"
  "$base_url/$mac_x64_archive"
  "$mac_x64_hash"
  "$base_url/$linux_arm_archive"
  "$linux_arm_hash"
  "$base_url/$linux_x64_archive"
  "$linux_x64_hash"
  "$base_url/$windows_x64_archive"
  "$windows_x64_hash"
)

check_template() {
  local template="$1"
  shift
  [ -f "$template" ] && [ ! -L "$template" ] \
    || fail "template must be a regular non-symlink file: $template"
  reject_text_encoding "$template"
  local spec token expected actual
  for spec in "$@"; do
    token="${spec%=*}"
    expected="${spec##*=}"
    actual="$(
      awk -v token="$token" '
        {
          line = $0
          while ((position = index(line, token)) > 0) {
            count++
            line = substr(line, position + length(token))
          }
        }
        END { print count + 0 }
      ' "$template"
    )"
    [ "$actual" -eq "$expected" ] \
      || fail "template token count mismatch: $(basename "$template") $token expected=$expected actual=$actual"
  done
  allowed="$tmp_root/allowed-tokens.txt"
  actual_tokens="$tmp_root/actual-tokens.txt"
  : >"$allowed"
  for spec in "$@"; do
    printf '%s\n' "${spec%=*}" >>"$allowed"
  done
  LC_ALL=C sort -u "$allowed" -o "$allowed"
  grep -oE '@@[A-Z0-9_]+@@' "$template" | LC_ALL=C sort -u >"$actual_tokens"
  cmp -s "$allowed" "$actual_tokens" \
    || fail "template contains an unknown or missing token: $template"
}

render_template() {
  local template="$1"
  local destination="$2"
  cp "$template" "$tmp_root/rendered"
  local index token value escaped next
  index=0
  while [ "$index" -lt "${#tokens[@]}" ]; do
    token="${tokens[$index]}"
    value="${values[$index]}"
    escaped="${value//\\/\\\\}"
    escaped="${escaped//&/\\&}"
    escaped="${escaped//|/\\|}"
    next="$tmp_root/rendered-next"
    sed "s|$token|$escaped|g" "$tmp_root/rendered" >"$next"
    mv "$next" "$tmp_root/rendered"
    index=$((index + 1))
  done
  if grep -Eq '@@[A-Z0-9_]+@@' "$tmp_root/rendered"; then
    fail "rendered template contains an unresolved token: $template"
  fi
  mkdir -p "$(dirname "$destination")"
  cp "$tmp_root/rendered" "$destination"
}

formula_template="$templates/rpotato.rb.in"
scoop_template="$templates/rpotato.json.in"
winget_version_template="$templates/winget-version.yaml.in"
winget_locale_template="$templates/winget-locale.yaml.in"
winget_installer_template="$templates/winget-installer.yaml.in"

check_template "$formula_template" \
  "@@DESCRIPTION@@=1" "@@HOMEPAGE@@=1" "@@VERSION@@=2" "@@LICENSE@@=1" \
  "@@URL_MAC_ARM@@=1" "@@SHA_MAC_ARM@@=1" \
  "@@URL_MAC_X64@@=1" "@@SHA_MAC_X64@@=1" \
  "@@URL_LINUX_ARM@@=1" "@@SHA_LINUX_ARM@@=1" \
  "@@URL_LINUX_X64@@=1" "@@SHA_LINUX_X64@@=1"
check_template "$scoop_template" \
  "@@VERSION@@=1" "@@DESCRIPTION@@=1" "@@HOMEPAGE@@=2" "@@LICENSE@@=1" \
  "@@URL_WINDOWS_X64@@=1" "@@SHA_WINDOWS_X64@@=1"
check_template "$winget_version_template" \
  "@@WINGET_ID@@=1" "@@VERSION@@=1" "@@WINGET_LOCALE@@=1"
check_template "$winget_locale_template" \
  "@@WINGET_ID@@=1" "@@VERSION@@=1" "@@WINGET_LOCALE@@=1" \
  "@@PUBLISHER@@=1" "@@PACKAGE_NAME@@=1" "@@LICENSE@@=1" \
  "@@DESCRIPTION@@=1" "@@HOMEPAGE@@=1"
check_template "$winget_installer_template" \
  "@@WINGET_ID@@=1" "@@VERSION@@=1" \
  "@@URL_WINDOWS_X64@@=1" "@@SHA_WINDOWS_X64@@=1"

mkdir -p "$output_root"
winget_output="$output_root/winget/manifests/m/MCprotein/rpotato/$version"
render_template "$formula_template" \
  "$output_root/homebrew/Formula/rpotato.rb"
render_template "$scoop_template" \
  "$output_root/scoop/bucket/rpotato.json"
render_template "$winget_version_template" \
  "$winget_output/MCprotein.rpotato.yaml"
render_template "$winget_locale_template" \
  "$winget_output/MCprotein.rpotato.locale.en-US.yaml"
render_template "$winget_installer_template" \
  "$winget_output/MCprotein.rpotato.installer.yaml"

printf 'package-manager manifests generated: tag=%s output=%s files=5\n' \
  "$tag" "$output_root"
