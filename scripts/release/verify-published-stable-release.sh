#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'published stable release error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 1 ] \
  || fail "usage: scripts/release/verify-published-stable-release.sh TAG"

tag="$1"
[[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || fail "tag must be a strict stable semver: $tag"
command -v jq >/dev/null 2>&1 || fail "jq is required"

release_json="$(cat)"
[ -n "$release_json" ] || fail "release metadata is empty"

jq -e \
  --arg tag "$tag" \
  '
    type == "object"
    and .tagName == $tag
    and .isDraft == false
    and .isPrerelease == false
    and (.publishedAt | type == "string")
    and (.publishedAt | test(
      "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"
    ))
  ' <<<"$release_json" >/dev/null \
  || fail "release must match the requested published non-draft stable tag: $tag"

printf 'published stable release ok: tag=%s\n' "$tag"
