#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  printf 'usage: %s <failure-cause> <upstream-results> <release-branch> [remote]\n' "$0" >&2
  exit 2
fi

failure_cause="$1"
upstream_results="$2"
release_branch="$3"
remote="${4:-origin}"

[[ "$failure_cause" =~ ^(test|build|checksums|published-assets-verify):(failure|cancelled|skipped|unknown)$ ]] \
  || { printf 'release failure reporter rejected cause: %s\n' "$failure_cause" >&2; exit 2; }
[[ "$upstream_results" =~ ^test=(success|failure|cancelled|skipped|unknown),build=(success|failure|cancelled|skipped|unknown),checksums=(success|failure|cancelled|skipped|unknown),published-assets-verify=(success|failure|cancelled|skipped|unknown)$ ]] \
  || { printf 'release failure reporter rejected upstream results\n' >&2; exit 2; }

if [[ ! "$release_branch" =~ ^release/v[0-9]+\.[0-9]+\.[0-9]+(-alpha\.[0-9]+)?$ ]]; then
  printf 'release failure reporter rejected branch: %s\n' "$release_branch" >&2
  exit 2
fi

remote_status='unavailable'
branch_status='unverifiable'
action='remote 상태를 확인할 수 없어 릴리스 브랜치 보존 여부를 확정하지 못했습니다.'
next='remote 접근을 복구한 뒤 릴리스 브랜치를 확인하고 실패한 워크플로를 다시 실행하세요.'
diagnostic_exit=1
if git ls-remote "$remote" HEAD >/dev/null 2>&1; then
  remote_status='reachable'
  if git ls-remote --exit-code --heads "$remote" "$release_branch" >/dev/null 2>&1; then
    branch_status='preserved'
    action='릴리스 브랜치가 보존된 것을 확인했습니다.'
    next='실패 원인 job을 확인하고 워크플로를 다시 실행하세요.'
    diagnostic_exit=0
  else
    branch_status='missing'
    action='릴리스 브랜치가 remote에 없어 보존을 확인하지 못했습니다.'
    next='release branch 정책과 삭제 이력을 조사한 뒤 브랜치를 복구하세요.'
  fi
fi

printf '릴리스 검증 실패\n- code: release.workflow.failed\n- cause: %s\n- upstream: %s\n- branch: %s\n- branch-status: %s\n- remote-status: %s\n- 동작: %s\n- 다음: %s\n' \
  "$failure_cause" "$upstream_results" "$release_branch" "$branch_status" "$remote_status" \
  "$action" "$next" >&2
exit "$diagnostic_exit"
