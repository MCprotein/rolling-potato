#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release asset verification error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 2 ] || fail "usage: scripts/release/verify-release-assets.sh TAG DOWNLOAD_DIR"

tag="$1"
download_dir="$2"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta|rc)\.[0-9]+)?$ ]] \
  || fail "invalid release tag: $tag"
[ -d "$download_dir" ] || fail "download directory is missing: $download_dir"

archives=(
  "rpotato-${tag}-aarch64-apple-darwin.tar.gz"
  "rpotato-${tag}-x86_64-apple-darwin.tar.gz"
  "rpotato-${tag}-x86_64-unknown-linux-gnu.tar.gz"
  "rpotato-${tag}-aarch64-unknown-linux-gnu.tar.gz"
  "rpotato-${tag}-x86_64-pc-windows-msvc.zip"
)
aggregate="rpotato-${tag}-checksums.txt"

is_expected_name() {
  local name="$1"
  [ "$name" = "$aggregate" ] && return 0
  local archive
  for archive in "${archives[@]}"; do
    [ "$name" = "$archive" ] && return 0
    [ "$name" = "${archive}.sha256" ] && return 0
  done
  return 1
}

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    fail "sha256sum or shasum is required"
  fi
}

reject_bom_or_crlf() {
  local path="$1"
  local prefix
  prefix="$(od -An -tx1 -N3 "$path" | tr -d ' \n')"
  [ "$prefix" != "efbbbf" ] || fail "UTF-8 BOM is forbidden: $(basename "$path")"
  if LC_ALL=C grep -q $'\r' "$path"; then
    fail "CRLF or CR bytes are forbidden: $(basename "$path")"
  fi
  local last
  last="$(tail -c 1 "$path" | od -An -tx1 | tr -d ' \n')"
  [ "$last" = "0a" ] || fail "file must end with exactly LF: $(basename "$path")"
}

shopt -s nullglob
entries=("$download_dir"/* "$download_dir"/.[!.]* "$download_dir"/..?*)
[ "${#entries[@]}" -eq 11 ] \
  || fail "expected exactly 11 release assets, found ${#entries[@]}"
for path in "${entries[@]}"; do
  name="$(basename "$path")"
  is_expected_name "$name" || fail "unexpected release asset: $name"
  [ -f "$path" ] && [ ! -L "$path" ] || fail "asset must be a regular non-symlink file: $name"
done

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-release-assets.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
expected_aggregate="$tmp_dir/expected-checksums.txt"
: >"$expected_aggregate"

for archive in "${archives[@]}"; do
  archive_path="$download_dir/$archive"
  sidecar_path="$download_dir/${archive}.sha256"
  [ -f "$archive_path" ] && [ ! -L "$archive_path" ] \
    || fail "missing regular archive: $archive"
  [ -f "$sidecar_path" ] && [ ! -L "$sidecar_path" ] \
    || fail "missing regular checksum sidecar: ${archive}.sha256"
  reject_bom_or_crlf "$sidecar_path"
  [ "$(wc -l <"$sidecar_path" | tr -d ' ')" -eq 1 ] \
    || fail "checksum sidecar must contain exactly one line: ${archive}.sha256"
  IFS= read -r sidecar_line <"$sidecar_path"
  [[ "$sidecar_line" =~ ^([0-9A-Fa-f]{64})\ \ (.+)$ ]] \
    || fail "malformed checksum sidecar: ${archive}.sha256"
  sidecar_hash="${BASH_REMATCH[1]}"
  sidecar_name="${BASH_REMATCH[2]}"
  [ "$sidecar_name" = "$archive" ] \
    || fail "checksum sidecar basename mismatch: ${archive}.sha256"
  actual_hash="$(sha256_file "$archive_path")"
  sidecar_hash_lower="$(printf '%s' "$sidecar_hash" | tr 'A-F' 'a-f')"
  actual_hash_lower="$(printf '%s' "$actual_hash" | tr 'A-F' 'a-f')"
  [ "$sidecar_hash_lower" = "$actual_hash_lower" ] \
    || fail "archive checksum mismatch: $archive"
  printf '%s  %s\n' "$sidecar_hash_lower" "$archive" >>"$expected_aggregate"
done

LC_ALL=C sort -t ' ' -k3,3 "$expected_aggregate" -o "$expected_aggregate"
aggregate_path="$download_dir/$aggregate"
[ -f "$aggregate_path" ] && [ ! -L "$aggregate_path" ] \
  || fail "missing regular aggregate checksum file: $aggregate"
reject_bom_or_crlf "$aggregate_path"
[ "$(wc -l <"$aggregate_path" | tr -d ' ')" -eq 5 ] \
  || fail "aggregate checksum file must contain exactly five lines"
normalized_aggregate="$tmp_dir/normalized-checksums.txt"
: >"$normalized_aggregate"
while IFS= read -r aggregate_line; do
  [[ "$aggregate_line" =~ ^([0-9A-Fa-f]{64})\ \ (.+)$ ]] \
    || fail "malformed aggregate checksum line"
  aggregate_hash_lower="$(printf '%s' "${BASH_REMATCH[1]}" | tr 'A-F' 'a-f')"
  printf '%s  %s\n' "$aggregate_hash_lower" "${BASH_REMATCH[2]}" >>"$normalized_aggregate"
done <"$aggregate_path"
cmp -s "$expected_aggregate" "$normalized_aggregate" \
  || fail "aggregate checksums are missing, duplicated, unsorted, or inconsistent"

printf 'release assets ok: tag=%s directory=%s archives=5 sidecars=5 aggregate=1\n' \
  "$tag" "$download_dir"
