#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release policy error: %s\n' "$1" >&2
  exit 1
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

version="$(
  sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1
)"

if [ -z "$version" ]; then
  fail "Cargo.toml package version was not found"
fi

semver_re='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$'

if ! [[ "$version" =~ $semver_re ]]; then
  fail "Cargo.toml version must be SemVer-like: $version"
fi

expected_tag="v$version"
expected_branch="release/$expected_tag"

branch="${RPOTATO_RELEASE_BRANCH:-${GITHUB_HEAD_REF:-}}"
tag="${RPOTATO_RELEASE_TAG:-}"

if [ -z "$tag" ] && [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
  tag="${GITHUB_REF_NAME:-}"
fi

if [ -z "$branch" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
fi

if [ -n "$tag" ]; then
  if [ "$tag" != "$expected_tag" ]; then
    fail "release tag must match Cargo.toml version: expected $expected_tag, got $tag"
  fi

  if [ "${RPOTATO_REQUIRE_RELEASE_BRANCH_DELETED:-0}" = "1" ]; then
    if git ls-remote --exit-code --heads origin "$expected_branch" >/dev/null 2>&1; then
      fail "remote release branch still exists after tagging: $expected_branch"
    fi
  fi
fi

if [ "${RPOTATO_REQUIRE_RELEASE_BRANCH:-0}" = "1" ]; then
  if [ "$branch" != "$expected_branch" ]; then
    fail "release PR branch must be $expected_branch, got $branch"
  fi
fi

if [[ "$branch" == release/* ]] && [ "$branch" != "$expected_branch" ]; then
  fail "release branch must match Cargo.toml version: expected $expected_branch, got $branch"
fi

printf 'release policy ok: version=%s branch=%s tag=%s\n' "$version" "${branch:-none}" "${tag:-none}"
