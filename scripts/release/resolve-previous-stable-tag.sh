#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'previous stable tag resolution error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 1 ] \
  || fail "usage: scripts/release/resolve-previous-stable-tag.sh CURRENT_TAG"

current_tag="$1"
stable_tag='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
[[ "$current_tag" =~ $stable_tag ]] \
  || fail "current tag must be a strict stable semver: $current_tag"
git rev-parse --quiet --verify "refs/tags/${current_tag}^{commit}" >/dev/null \
  || fail "current tag does not exist: $current_tag"

semver_less() {
  local left="$1"
  local right="$2"
  local left_version right_version
  local left_major left_minor left_patch
  local right_major right_minor right_patch
  left_version="${left#v}"
  right_version="${right#v}"
  IFS=. read -r left_major left_minor left_patch <<<"$left_version"
  IFS=. read -r right_major right_minor right_patch <<<"$right_version"
  if [ "$left_major" -ne "$right_major" ]; then
    [ "$left_major" -lt "$right_major" ]
  elif [ "$left_minor" -ne "$right_minor" ]; then
    [ "$left_minor" -lt "$right_minor" ]
  else
    [ "$left_patch" -lt "$right_patch" ]
  fi
}

best=""
while IFS= read -r candidate; do
  [[ "$candidate" =~ $stable_tag ]] || continue
  [ "$candidate" != "$current_tag" ] || continue
  semver_less "$candidate" "$current_tag" || continue
  git merge-base --is-ancestor \
    "refs/tags/${candidate}^{commit}" "refs/tags/${current_tag}^{commit}" \
    || continue
  if [ -z "$best" ] || semver_less "$best" "$candidate"; then
    best="$candidate"
  fi
done < <(git tag --list)

[ -n "$best" ] \
  || fail "no ancestral stable tag exists below current tag: $current_tag"
printf '%s\n' "$best"
