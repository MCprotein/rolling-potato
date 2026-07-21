#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
verifier="$script_dir/verify-release-assets.sh"
manifest="$script_dir/../../config/release-targets.tsv"
tag="v0.34.0"
root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-release-assets-test.XXXXXX")"
trap 'rm -rf "$root"' EXIT

archives=()
while IFS=$'\t' read -r _os _arch target _binary archive _runner; do
  archives+=("rpotato-${tag}-${target}.${archive}")
done < <(awk -F '\t' '!/^[[:space:]]*#/ && !/^[[:space:]]*$/ { print }' "$manifest")

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

build_valid() {
  local directory="$1"
  mkdir -p "$directory"
  local archive digest
  for archive in "${archives[@]}"; do
    printf 'fixture:%s\n' "$archive" >"$directory/$archive"
    digest="$(sha256_file "$directory/$archive")"
    printf '%s  %s\n' "$digest" "$archive" >"$directory/${archive}.sha256"
  done
  local aggregate_tmp="$directory/.aggregate.tmp"
  : >"$aggregate_tmp"
  for archive in "${archives[@]}"; do
    cat "$directory/${archive}.sha256" >>"$aggregate_tmp"
  done
  LC_ALL=C sort -t ' ' -k3,3 "$aggregate_tmp" \
    >"$directory/rpotato-${tag}-checksums.txt"
  rm -f "$aggregate_tmp"
}

expect_failure() {
  local case_name="$1"
  local directory="$2"
  if "$verifier" "$tag" "$directory" >/dev/null 2>&1; then
    printf 'release asset fixture unexpectedly passed: %s\n' "$case_name" >&2
    exit 1
  fi
  printf 'release asset fixture passed: %s\n' "$case_name"
}

expect_valid_tag_shape() {
  local candidate="$1"
  local output
  output="$("$verifier" "$candidate" "$root/not-present" 2>&1 || true)"
  if [[ "$output" == *"invalid release tag"* ]] \
    || [[ "$output" != *"download directory is missing"* ]]; then
    printf 'release asset tag fixture unexpectedly failed: %s\n' "$candidate" >&2
    exit 1
  fi
  printf 'release asset tag fixture passed: %s\n' "$candidate"
}

expect_valid_tag_shape "v0.34.0-alpha.1"
expect_valid_tag_shape "v0.34.0-beta.2"
expect_valid_tag_shape "v0.34.0-rc.3"

success="$root/success"
build_valid "$success"
"$verifier" "$tag" "$success" >/dev/null
printf 'release asset fixture passed: success\n'

uppercase="$root/uppercase"
cp -R "$success" "$uppercase"
for archive in "${archives[@]}"; do
  sidecar="$uppercase/${archive}.sha256"
  digest="$(awk '{print toupper($1)}' "$sidecar")"
  printf '%s  %s\n' "$digest" "$archive" >"$sidecar"
done
"$verifier" "$tag" "$uppercase" >/dev/null
printf 'release asset fixture passed: uppercase\n'

missing="$root/missing"
cp -R "$success" "$missing"
rm -f "$missing/${archives[0]}"
expect_failure missing "$missing"

extra="$root/extra"
cp -R "$success" "$extra"
printf 'extra\n' >"$extra/unexpected.txt"
expect_failure extra "$extra"

duplicate="$root/duplicate"
cp -R "$success" "$duplicate"
head -n 1 "$duplicate/rpotato-${tag}-checksums.txt" \
  >>"$duplicate/rpotato-${tag}-checksums.txt"
expect_failure duplicate "$duplicate"

for mutation in wrong_digest wrong_basename crlf bom; do
  bad_sidecar="$root/bad-sidecar-$mutation"
  cp -R "$success" "$bad_sidecar"
  sidecar="$bad_sidecar/${archives[0]}.sha256"
  line="$(tr -d '\n' <"$sidecar")"
  case "$mutation" in
    wrong_digest)
      printf '%064d  %s\n' 0 "${archives[0]}" >"$sidecar"
      ;;
    wrong_basename)
      digest="$(awk '{print $1}' "$sidecar")"
      printf '%s  %s\n' "$digest" "wrong-name.tar.gz" >"$sidecar"
      ;;
    crlf)
      printf '%s\r\n' "$line" >"$sidecar"
      ;;
    bom)
      printf '\357\273\277%s\n' "$line" >"$sidecar"
      ;;
  esac
  expect_failure "bad_sidecar/$mutation" "$bad_sidecar"
done

for mutation in wrong_digest wrong_order wrong_count crlf bom; do
  bad_aggregate="$root/bad-aggregate-$mutation"
  cp -R "$success" "$bad_aggregate"
  aggregate="$bad_aggregate/rpotato-${tag}-checksums.txt"
  original="$root/aggregate-$mutation.original"
  cp "$aggregate" "$original"
  case "$mutation" in
    wrong_digest)
      awk 'NR == 1 { $1 = sprintf("%064d", 0) } { print }' "$original" >"$aggregate"
      ;;
    wrong_order)
      { sed -n '2p' "$original"; sed -n '1p' "$original"; sed -n '3,5p' "$original"; } >"$aggregate"
      ;;
    wrong_count)
      sed -n '1,4p' "$original" >"$aggregate"
      ;;
    crlf)
      sed 's/$/\r/' "$original" >"$aggregate"
      ;;
    bom)
      printf '\357\273\277' >"$aggregate"
      sed -n '1,5p' "$original" >>"$aggregate"
      ;;
  esac
  expect_failure "bad_aggregate/$mutation" "$bad_aggregate"
done

printf 'release asset verifier fixtures ok: success uppercase missing extra duplicate bad_sidecar bad_aggregate\n'
