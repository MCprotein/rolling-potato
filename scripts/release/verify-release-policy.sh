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
remote_branch_ref="refs/remotes/origin/$expected_branch"

branch="${RPOTATO_RELEASE_BRANCH:-${GITHUB_HEAD_REF:-}}"
tag="${RPOTATO_RELEASE_TAG:-}"
require_release_branch="${RPOTATO_REQUIRE_RELEASE_BRANCH:-0}"

if [ -z "$tag" ] && [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
  tag="${GITHUB_REF_NAME:-}"
fi

if [ -z "$branch" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
fi

if [ "$require_release_branch" = "auto" ]; then
  require_release_branch=0
  if [[ "$branch" == release/* ]]; then
    require_release_branch=1
  elif [ -n "${RPOTATO_RELEASE_BASE_REF:-}" ]; then
    base_version="$(
      git show "${RPOTATO_RELEASE_BASE_REF}:Cargo.toml" 2>/dev/null \
        | sed -n 's/^version = "\([^"]*\)"/\1/p' \
        | head -n 1
    )"
    if [ -z "$base_version" ]; then
      fail "base Cargo.toml package version was not found: ${RPOTATO_RELEASE_BASE_REF}"
    fi
    if [ "$version" != "$base_version" ]; then
      require_release_branch=1
    fi
  fi
elif [ "$require_release_branch" != "0" ] \
  && [ "$require_release_branch" != "1" ]; then
  fail "RPOTATO_REQUIRE_RELEASE_BRANCH must be 0, 1, or auto"
fi

if [ -n "$tag" ]; then
  if [ "$tag" != "$expected_tag" ]; then
    fail "release tag must match Cargo.toml version: expected $expected_tag, got $tag"
  fi

  tag_commit="$(
    git rev-parse "$tag^{commit}" 2>/dev/null || git rev-parse HEAD
  )"

  if [ "${RPOTATO_REQUIRE_TAG_ON_MAIN:-0}" = "1" ]; then
    git fetch --quiet origin main
    if ! git merge-base --is-ancestor "$tag_commit" origin/main; then
      fail "release tag commit must be on origin/main: $tag"
    fi
  fi

  if git ls-remote --exit-code --heads origin "$expected_branch" >/dev/null 2>&1; then
    git fetch --quiet origin "$expected_branch:$remote_branch_ref"
    release_branch_commit="$(git rev-parse "$remote_branch_ref")"
    if ! git merge-base --is-ancestor "$release_branch_commit" "$tag_commit" \
      && ! git diff --quiet "$release_branch_commit" "$tag_commit"; then
      fail "release branch must be merged or squash-tree-equivalent before tagging: $expected_branch"
    fi

    if [ "${RPOTATO_DELETE_RELEASE_BRANCH:-0}" = "1" ]; then
      if [ "${RPOTATO_DRY_RUN_DELETE:-0}" = "1" ]; then
        printf 'release policy dry-run: would delete remote branch %s\n' "$expected_branch"
      else
        git push origin --delete "$expected_branch"
      fi
    fi
  elif [ "${RPOTATO_REQUIRE_RELEASE_BRANCH_EXISTS:-0}" = "1" ]; then
    fail "matching remote release branch was not found: $expected_branch"
  fi
fi

if [ "$require_release_branch" = "1" ]; then
  if [ "$branch" != "$expected_branch" ]; then
    fail "release PR branch must be $expected_branch, got $branch"
  fi
fi

if [[ "$branch" == release/* ]] && [ "$branch" != "$expected_branch" ]; then
  fail "release branch must match Cargo.toml version: expected $expected_branch, got $branch"
fi

printf 'release policy ok: version=%s branch=%s tag=%s\n' "$version" "${branch:-none}" "${tag:-none}"
