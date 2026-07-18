#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release workflow contract error: %s\n' "$1" >&2
  exit 1
}

release_workflow=".github/workflows/release-binaries.yml"
windows_targeted_workflow=".github/workflows/windows-native-targeted.yml"
policy_workflow=".github/workflows/release-policy.yml"
candidate_workflow=".github/workflows/refactor-candidate.yml"
candidate_preflight="scripts/ci/verify-pr-candidate-preflight.sh"
checkout_pin='actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0'

job_block() {
  local workflow job
  if [ "$#" -eq 1 ]; then
    workflow="$release_workflow"
    job="$1"
  else
    workflow="$1"
    job="$2"
  fi
  awk -v job="$job" '
    $0 == "  " job ":" { active = 1 }
    active && $0 ~ /^  [A-Za-z0-9_-]+:$/ && $0 != "  " job ":" { exit }
    active { print }
  ' "$workflow"
}

step_block() {
  local workflow="$1"
  local step="$2"
  awk -v step="$step" '
    $0 == "      - name: " step { active = 1 }
    active && $0 ~ /^      - name: / && $0 != "      - name: " step { exit }
    active { print }
  ' "$workflow"
}

windows_preflight_commands() {
  sed -n 's/^[[:space:]]*\(cargo test --locked --target .*\)$/\1/p' \
    | sed 's/\${{ matrix.target }}/x86_64-pc-windows-msvc/g'
}

require_line() {
  local body="$1"
  local line="$2"
  grep -F -- "$line" <<<"$body" >/dev/null || fail "missing contract line: $line"
}

policy_body="$(cat "$policy_workflow")"
candidate_body="$(cat "$candidate_workflow")"
candidate_preflight_body="$(cat "$candidate_preflight")"
candidate_windows="$(job_block "$candidate_workflow" windows-compile)"
candidate_permissions="$(awk '
  /^permissions:$/ { active = 1 }
  active && NR != 1 && /^[^[:space:]]/ && $0 != "permissions:" { exit }
  active { print }
' "$candidate_workflow")"
[ "$candidate_permissions" = $'permissions:\n  contents: read' ] \
  || fail "candidate workflow permissions must be exactly contents: read"
require_line "$candidate_body" '      - ready_for_review'
require_line "$candidate_body" "    if: github.event.pull_request.draft == false && contains(github.event.pull_request.labels.*.name, 'release-candidate')"
require_line "$candidate_body" '    outputs:'
require_line "$candidate_body" '      candidate_sha: ${{ steps.candidate.outputs.candidate_sha }}'
require_line "$candidate_body" '      CANDIDATE_SHA: ${{ github.event.pull_request.head.sha }}'
require_line "$candidate_body" '          persist-credentials: false'
require_line "$candidate_body" '          ref: ${{ github.event.pull_request.head.sha }}'
require_line "$candidate_body" '        id: candidate'
require_line "$candidate_body" '          actual_sha="$(git rev-parse HEAD)"'
require_line "$candidate_body" '          if [ "$actual_sha" != "$CANDIDATE_SHA" ]; then'
require_line "$candidate_body" "            printf 'candidate SHA mismatch: expected %s, got %s\\n' \"\$CANDIDATE_SHA\" \"\$actual_sha\" >&2"
require_line "$candidate_body" '            exit 1'
require_line "$candidate_body" '        run: cargo test --locked -- --test-threads=1'
require_line "$candidate_body" '        run: cargo clippy --locked --all-targets --all-features -- -D warnings'
require_line "$candidate_body" '        run: cargo build --locked --release'
require_line "$candidate_body" '        run: scripts/performance/verify-v0.39-workflow-budgets.sh'
require_line "$candidate_body" '  windows-compile:'
require_line "$candidate_windows" "    if: github.event.pull_request.draft == false && contains(github.event.pull_request.labels.*.name, 'release-candidate')"
require_line "$candidate_windows" '    runs-on: windows-2025'
require_line "$candidate_windows" '      CANDIDATE_SHA: ${{ github.event.pull_request.head.sha }}'
require_line "$candidate_windows" "        uses: $checkout_pin"
require_line "$candidate_windows" '          persist-credentials: false'
require_line "$candidate_windows" '          ref: ${{ github.event.pull_request.head.sha }}'
require_line "$candidate_windows" '          actual_sha="$(git rev-parse HEAD)"'
require_line "$candidate_windows" '          if [ "$actual_sha" != "$CANDIDATE_SHA" ]; then'
require_line "$candidate_windows" '        run: rustup target add x86_64-pc-windows-msvc'
require_line "$candidate_windows" '        run: cargo check --locked --target x86_64-pc-windows-msvc --all-targets --all-features'
[ -x "$candidate_preflight" ] || fail "candidate preflight must be executable"
require_line "$candidate_preflight_body" 'scripts/release/verify-toolchain-pins.sh'
require_line "$candidate_preflight_body" 'cargo fmt --all -- --check'
require_line "$candidate_preflight_body" 'cargo test --locked --test architecture_contract migration_map_recursively_covers_every_governed_file_and_exact_slice -- --exact --test-threads=1'
require_line "$candidate_preflight_body" 'cargo clippy --locked --all-targets --all-features -- -D warnings'
require_line "$candidate_preflight_body" 'bash scripts/release/test-release-workflow-contract.sh'
release_windows_preflight="$(
  step_block "$release_workflow" "Test native Windows backend lifecycle" \
    | windows_preflight_commands
)"
targeted_windows_preflight="$(
  step_block "$windows_targeted_workflow" "Test backend lifecycle" \
    | windows_preflight_commands
)"
[ "$(printf '%s\n' "$release_windows_preflight" | awk 'NF { count++ } END { print count + 0 }')" -eq 5 ] \
  || fail "release Windows preflight must contain exactly five cargo test commands"
[ "$targeted_windows_preflight" = "$release_windows_preflight" ] \
  || fail "targeted Windows preflight must exactly match the release workflow"
require_line "$policy_body" 'RPOTATO_RELEASE_BASE_REF: ${{ github.event_name == '\''pull_request'\'' && format('\''origin/{0}'\'', github.base_ref) || '\'''\'' }}'
require_line "$policy_body" 'RPOTATO_REQUIRE_RELEASE_BRANCH: ${{ github.event_name == '\''pull_request'\'' && '\''auto'\'' || '\''0'\'' }}'
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

delete_count="$(
  cat "$release_workflow" \
    | awk '/RPOTATO_DELETE_RELEASE_BRANCH:/ { count++ } END { print count + 0 }'
)"
[ "$delete_count" -eq 1 ] || fail "binary release workflow must have exactly one delete owner"
grep -x '      RPOTATO_DELETE_RELEASE_BRANCH: 1' "$release_workflow" >/dev/null \
  || fail "delete owner must be the binary release cleanup job-level env literal"
if grep -En 'export[[:space:]]+RPOTATO_DELETE_RELEASE_BRANCH|^[[:space:]]{10,}RPOTATO_DELETE_RELEASE_BRANCH:|RPOTATO_DELETE_RELEASE_BRANCH:.*\$\{\{' "$release_workflow" >/dev/null; then
  fail "step-scoped, exported, or dynamic delete owner is forbidden"
fi
require_line "$cleanup" '      RPOTATO_DELETE_RELEASE_BRANCH: 1'

require_line "$cleanup" '          fetch-depth: 0'
for need in test build checksums published-assets-verify; do
  require_line "$cleanup" "      - $need"
done
require_line "$preserve" '          fetch-depth: 0'
for need in test build checksums published-assets-verify; do
  require_line "$preserve" "      - $need"
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

release_policy_accepts_squash_merged_tree() {
  local fixture remote seed pretag_output output
  fixture="$(mktemp -d)"
  remote="$fixture/remote.git"
  seed="$fixture/seed"
  pretag_output="$fixture/pretag-output"
  output="$fixture/output"
  git init --bare --quiet "$remote"
  git init --initial-branch=main --quiet "$seed"
  git -C "$seed" config user.name release-contract
  git -C "$seed" config user.email release-contract@example.invalid
  mkdir -p "$seed/scripts/release"
  printf '[package]\nname = "release-policy-fixture"\nversion = "0.34.0"\n' \
    >"$seed/Cargo.toml"
  cp scripts/release/verify-release-policy.sh "$seed/scripts/release/verify-release-policy.sh"
  git -C "$seed" add Cargo.toml scripts/release/verify-release-policy.sh
  git -C "$seed" commit --quiet -m 'test: release policy base'
  git -C "$seed" remote add origin "$remote"
  git -C "$seed" push --quiet origin main
  git -C "$seed" checkout --quiet -b release/v0.34.0
  printf 'release tree\n' >"$seed/release.txt"
  git -C "$seed" add release.txt
  git -C "$seed" commit --quiet -m 'test: release tree'
  git -C "$seed" push --quiet origin release/v0.34.0
  git -C "$seed" checkout --quiet main
  git -C "$seed" merge --quiet --squash release/v0.34.0 >/dev/null 2>&1
  git -C "$seed" commit --quiet -m 'test: squash release tree'
  git -C "$seed" push --quiet origin main

  (
    cd "$seed"
    RPOTATO_RELEASE_BRANCH=main \
      RPOTATO_RELEASE_TAG=v0.34.0 \
      RPOTATO_REQUIRE_TAG_ON_MAIN=1 \
      RPOTATO_REQUIRE_RELEASE_BRANCH_EXISTS=1 \
      scripts/release/verify-release-policy.sh
  ) >"$pretag_output"
  grep -F -- 'release policy ok: version=0.34.0 branch=main tag=v0.34.0' \
    "$pretag_output" >/dev/null || fail "pre-tag main HEAD fallback was rejected"

  git -C "$seed" tag v0.34.0
  (
    cd "$seed"
    RPOTATO_RELEASE_BRANCH=main \
      RPOTATO_RELEASE_TAG=v0.34.0 \
      RPOTATO_REQUIRE_TAG_ON_MAIN=1 \
      RPOTATO_REQUIRE_RELEASE_BRANCH_EXISTS=1 \
      scripts/release/verify-release-policy.sh
  ) >"$output"
  grep -F -- 'release policy ok: version=0.34.0 branch=main tag=v0.34.0' "$output" \
    >/dev/null || fail "squash-merged release tree was rejected"
  rm -rf "$fixture"
}

release_policy_scopes_release_branches_to_version_changes() {
  local fixture same_output changed_stderr
  fixture="$(mktemp -d)"
  same_output="$fixture/same-output"
  changed_stderr="$fixture/changed-stderr"
  git init --initial-branch=main --quiet "$fixture/repo"
  git -C "$fixture/repo" config user.name release-contract
  git -C "$fixture/repo" config user.email release-contract@example.invalid
  mkdir -p "$fixture/repo/scripts/release"
  printf '[package]\nname = "release-policy-fixture"\nversion = "0.34.3"\n' \
    >"$fixture/repo/Cargo.toml"
  cp scripts/release/verify-release-policy.sh \
    "$fixture/repo/scripts/release/verify-release-policy.sh"
  git -C "$fixture/repo" add Cargo.toml scripts/release/verify-release-policy.sh
  git -C "$fixture/repo" commit --quiet -m 'test: release policy base'
  git -C "$fixture/repo" checkout --quiet -b docs/policy

  (
    cd "$fixture/repo"
    RPOTATO_RELEASE_TAG= \
      GITHUB_REF_TYPE= \
      GITHUB_REF_NAME= \
      RPOTATO_RELEASE_BRANCH=docs/policy \
      RPOTATO_RELEASE_BASE_REF=main \
      RPOTATO_REQUIRE_RELEASE_BRANCH=auto \
      scripts/release/verify-release-policy.sh
  ) >"$same_output"
  grep -F -- 'release policy ok: version=0.34.3 branch=docs/policy tag=none' \
    "$same_output" >/dev/null \
    || fail "ordinary pull request was rejected as release work"

  sed -i.bak 's/version = "0.34.3"/version = "0.34.4"/' \
    "$fixture/repo/Cargo.toml"
  if (
    cd "$fixture/repo"
    RPOTATO_RELEASE_TAG= \
      GITHUB_REF_TYPE= \
      GITHUB_REF_NAME= \
      RPOTATO_RELEASE_BRANCH=docs/policy \
      RPOTATO_RELEASE_BASE_REF=main \
      RPOTATO_REQUIRE_RELEASE_BRANCH=auto \
      scripts/release/verify-release-policy.sh
  ) 2>"$changed_stderr"; then
    fail "version-changing feature branch unexpectedly passed release policy"
  fi
  grep -F -- \
    'release policy error: release PR branch must be release/v0.34.4, got docs/policy' \
    "$changed_stderr" >/dev/null \
    || fail "version-changing feature branch emitted the wrong policy failure"
  rm -rf "$fixture"
}

durable_proof_selector_requires_exact_single_test() {
  local output
  output="$(mktemp)"
  # shellcheck source=scripts/release/verify-durable-runtime-proofs.sh
  source scripts/release/verify-durable-runtime-proofs.sh

  cargo() {
    printf 'running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored\n'
  }
  if run_proof zero-match --bin rpotato stale::selector >"$output" 2>&1; then
    fail "zero-match durable proof unexpectedly succeeded"
  fi
  grep -F -- 'did not execute exactly one passing test' "$output" >/dev/null \
    || fail "zero-match durable proof emitted the wrong failure"

  cargo() {
    printf 'running 2 tests\n\ntest result: ok. 2 passed; 0 failed; 0 ignored\n'
  }
  if run_proof multiple-match --bin rpotato broad::selector >"$output" 2>&1; then
    fail "multiple-match durable proof unexpectedly succeeded"
  fi

  cargo() {
    printf 'running 1 test\ntest exact::selector ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored\n'
  }
  run_proof exact-match --bin rpotato exact::selector >"$output" 2>&1 \
    || fail "single-match durable proof was rejected"
  grep -F -- 'release proof ok: exact-match' "$output" >/dev/null \
    || fail "single-match durable proof did not report success"
  rm -f "$output"
}

release_failure_diagnostic_is_exact_and_always_emitted
release_policy_accepts_squash_merged_tree
release_policy_scopes_release_branches_to_version_changes
durable_proof_selector_requires_exact_single_test

printf 'release workflow contract ok: asset-verified-cleanup preservation-failure-only\n'
