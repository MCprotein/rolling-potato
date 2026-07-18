#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'package-manager workflow input error: %s\n' "$1" >&2
  exit 1
}

[ "$#" -eq 3 ] \
  || fail "usage: scripts/release/validate-package-manager-workflow-inputs.sh MODE CURRENT_TAG PREVIOUS_TAG"

mode="$1"
current_tag="$2"
previous_tag="$3"
stable_tag='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

[[ "$current_tag" =~ $stable_tag ]] \
  || fail "current tag must be a strict stable semver: $current_tag"

case "$mode" in
  release)
    [ -z "$previous_tag" ] \
      || fail "release mode derives the previous tag and rejects caller input"
    ;;
  qualification)
    [ "$previous_tag" = "v0.38.0" ] && [ "$current_tag" = "v0.39.0" ] \
      || fail "qualification mode is pinned to v0.38.0 -> v0.39.0"
    ;;
  recovery)
    [ -z "$previous_tag" ] \
      || fail "recovery mode derives the previous tag and rejects caller input"
    ;;
  *)
    fail "mode must be release, qualification, or recovery: $mode"
    ;;
esac

printf 'package-manager workflow inputs ok: mode=%s current=%s previous=%s\n' \
  "$mode" "$current_tag" "${previous_tag:-derived}"
