#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'current package-manager generation error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] \
  || fail "usage: scripts/release/generate-current-package-manager-manifests.sh TAG CHECKSUM_FILE OUT"

tag="$1"
checksum_file="$2"
output_root="$3"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
version="${tag#v}"
cargo_version="$(
  sed -n 's/^version = "\([^"]*\)"/\1/p' "$repo_root/Cargo.toml" | head -n 1
)"
[ -n "$cargo_version" ] || fail "Cargo.toml package version was not found"
[ "$tag" = "v$version" ] && [ "$version" = "$cargo_version" ] \
  || fail "requested tag $tag does not match Cargo.toml version $cargo_version"

"$script_dir/generate-package-manager-manifests.sh" \
  "$tag" "$checksum_file" "$output_root"
"$script_dir/verify-package-manager-manifests.sh" \
  "$tag" "$checksum_file" "$output_root"

printf 'current package-manager candidate ok: tag=%s output=%s\n' \
  "$tag" "$output_root"
