#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release workflow contract error: %s\n' "$1" >&2
  exit 1
}

release_workflow=".github/workflows/release-binaries.yml"
policy_workflow=".github/workflows/release-policy.yml"
checkout_pin='actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0'

job_block() {
  local job="$1"
  awk -v job="$job" '
    $0 == "  " job ":" { active = 1 }
    active && $0 ~ /^  [A-Za-z0-9_-]+:$/ && $0 != "  " job ":" { exit }
    active { print }
  ' "$release_workflow"
}

require_line() {
  local body="$1"
  local line="$2"
  grep -F -- "$line" <<<"$body" >/dev/null || fail "missing contract line: $line"
}

policy_body="$(cat "$policy_workflow")"
require_line "$policy_body" 'RPOTATO_REQUIRE_RELEASE_BRANCH_EXISTS: ${{ github.ref_type == '\''tag'\'' && '\''1'\'' || '\''0'\'' }}'
require_line "$policy_body" 'RPOTATO_REQUIRE_TAG_ON_MAIN: ${{ github.ref_type == '\''tag'\'' && '\''1'\'' || '\''0'\'' }}'
require_line "$policy_body" 'RPOTATO_DELETE_RELEASE_BRANCH: "0"'
if grep -En 'git push .*--delete|RPOTATO_DELETE_RELEASE_BRANCH: (1|"1")' "$policy_workflow" >/dev/null; then
  fail "release-policy workflow must validate only and never own deletion"
fi

published="$(job_block published-assets-verify)"
cleanup="$(job_block cleanup-release-branch)"
preserve="$(job_block release-failure-preserves-branch)"
for body in "$published" "$cleanup" "$preserve"; do
  require_line "$body" "uses: $checkout_pin"
done
require_line "$published" 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY"'
require_line "$published" '--dir dist/published'
if grep -F -- '--pattern' <<<"$published" >/dev/null; then
  fail "published verification must download every release asset"
fi

delete_count="$(awk '/RPOTATO_DELETE_RELEASE_BRANCH:/ { count++ } END { print count + 0 }' "$release_workflow")"
[ "$delete_count" -eq 1 ] || fail "release workflow must have exactly one delete owner"
grep -x '      RPOTATO_DELETE_RELEASE_BRANCH: 1' "$release_workflow" >/dev/null \
  || fail "delete owner must be the cleanup job-level env literal"
if grep -En 'export[[:space:]]+RPOTATO_DELETE_RELEASE_BRANCH|^[[:space:]]{10,}RPOTATO_DELETE_RELEASE_BRANCH:|RPOTATO_DELETE_RELEASE_BRANCH:.*\$\{\{' "$release_workflow" >/dev/null; then
  fail "step-scoped, exported, or dynamic delete owner is forbidden"
fi
require_line "$cleanup" '      RPOTATO_DELETE_RELEASE_BRANCH: 1'

for body in "$cleanup" "$preserve"; do
  require_line "$body" '          fetch-depth: 0'
  for need in test build checksums published-assets-verify; do
    require_line "$body" "      - $need"
  done
done
require_line "$cleanup" "      github.event_name == 'release' &&"
require_line "$cleanup" "      needs.test.result == 'success' && needs.build.result == 'success' &&"
require_line "$cleanup" "      needs.checksums.result == 'success' &&"
require_line "$cleanup" "      needs.published-assets-verify.result == 'success'"
require_line "$preserve" "      always() && github.event_name == 'release' &&"
require_line "$preserve" "      (needs.test.result != 'success' || needs.build.result != 'success' ||"
require_line "$preserve" "      needs.checksums.result != 'success' ||"
require_line "$preserve" "      needs.published-assets-verify.result != 'success')"
require_line "$cleanup" 'scripts/release/verify-release-policy.sh'
require_line "$cleanup" 'git ls-remote --exit-code --heads origin "release/$RELEASE_TAG"'
require_line "$preserve" 'TEST_RESULT: ${{ needs.test.result }}'
require_line "$preserve" 'BUILD_RESULT: ${{ needs.build.result }}'
require_line "$preserve" 'CHECKSUMS_RESULT: ${{ needs.checksums.result }}'
require_line "$preserve" 'PUBLISHED_ASSETS_RESULT: ${{ needs.published-assets-verify.result }}'
require_line "$preserve" 'failure_cause="test:$TEST_RESULT"'
require_line "$preserve" 'failure_cause="build:$BUILD_RESULT"'
require_line "$preserve" 'failure_cause="checksums:$CHECKSUMS_RESULT"'
require_line "$preserve" 'failure_cause="published-assets-verify:$PUBLISHED_ASSETS_RESULT"'
require_line "$preserve" 'scripts/release/report-release-failure.sh "$failure_cause" "$upstream_results" "$RELEASE_BRANCH" origin'

release_failure_diagnostic_is_exact_and_always_emitted() {
  local fixture remote seed branch before after stdout stderr expected actual secret
  fixture="$(mktemp -d)"
  remote="$fixture/remote.git"
  seed="$fixture/seed"
  branch='release/v0.34.0'
  stdout="$fixture/stdout"
  stderr="$fixture/stderr"
  secret='RELEASE_DIAGNOSTIC_SECRET_MUST_NOT_LEAK_7341'
  git init --bare --quiet "$remote"
  git init --quiet "$seed"
  git -C "$seed" config user.name release-contract
  git -C "$seed" config user.email release-contract@example.invalid
  printf 'release branch sentinel\n' > "$seed/sentinel.txt"
  git -C "$seed" add sentinel.txt
  git -C "$seed" commit --quiet -m 'test: release branch sentinel'
  git -C "$seed" branch "$branch"
  git -C "$seed" remote add fixture "$remote"
  git -C "$seed" push --quiet fixture "$branch"
  before="$(git ls-remote --heads "$remote" "$branch")"
  upstream='test=success,build=success,checksums=failure,published-assets-verify=skipped'
  RPOTATO_RELEASE_TEST_SECRET="$secret" \
    scripts/release/report-release-failure.sh checksums:failure "$upstream" "$branch" "$remote" \
    >"$stdout" 2>"$stderr" \
    || fail "release failure reporter fixture failed"
  after="$(git ls-remote --heads "$remote" "$branch")"
  expected='릴리스 검증 실패
- code: release.workflow.failed
- cause: checksums:failure
- upstream: test=success,build=success,checksums=failure,published-assets-verify=skipped
- branch: release/v0.34.0
- branch-status: preserved
- remote-status: reachable
- 동작: 릴리스 브랜치가 보존된 것을 확인했습니다.
- 다음: 실패 원인 job을 확인하고 워크플로를 다시 실행하세요.'
  actual="$(cat "$stderr")"
  [ "$actual" = "$expected" ] || fail "ReleaseDiagnosticV1 fixture bytes changed"
  [ ! -s "$stdout" ] || fail "ReleaseDiagnosticV1 wrote unexpected stdout"
  [ "$before" = "$after" ] || fail "release failure reporter changed the release branch"
  if grep -F -- "$secret" "$stdout" "$stderr" >/dev/null; then
    fail "release failure diagnostic leaked environment secret"
  fi

  missing_branch='release/v0.34.1'
  if scripts/release/report-release-failure.sh test:failure \
    'test=failure,build=skipped,checksums=skipped,published-assets-verify=skipped' \
    "$missing_branch" "$remote" >"$stdout" 2>"$stderr"; then
    fail "missing release branch diagnostic unexpectedly succeeded"
  fi
  grep -F -- '- branch-status: missing' "$stderr" >/dev/null \
    || fail "missing branch diagnostic was not emitted"
  grep -F -- '- remote-status: reachable' "$stderr" >/dev/null \
    || fail "missing branch remote status was not emitted"

  if scripts/release/report-release-failure.sh build:failure \
    'test=success,build=failure,checksums=skipped,published-assets-verify=skipped' \
    "$branch" "$fixture/unavailable.git" >"$stdout" 2>"$stderr"; then
    fail "unavailable remote diagnostic unexpectedly succeeded"
  fi
  grep -F -- '- branch-status: unverifiable' "$stderr" >/dev/null \
    || fail "unverifiable branch diagnostic was not emitted"
  grep -F -- '- remote-status: unavailable' "$stderr" >/dev/null \
    || fail "unavailable remote diagnostic was not emitted"
  rm -rf "$fixture"
}

release_failure_diagnostic_is_exact_and_always_emitted

printf 'release workflow contract ok: cleanup-success-only preservation-failure-only\n'
