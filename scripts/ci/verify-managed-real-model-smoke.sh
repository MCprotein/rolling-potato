#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${1:-$repo_root/target/debug/rpotato}"

fail() {
  printf 'managed real-model smoke failed: %s\n' "$1" >&2
  exit 1
}

if [[ "${RPOTATO_REAL_MODEL_SMOKE:-0}" != "1" ]]; then
  printf '%s\n' \
    'managed real-model smoke is opt-in; set RPOTATO_REAL_MODEL_SMOKE=1' >&2
  exit 2
fi
[[ -x "$binary" ]] || fail "binary is not executable: $binary"

owned_project=""
if [[ -n "${RPOTATO_REAL_MODEL_PROJECT_ROOT:-}" ]]; then
  project_root="$RPOTATO_REAL_MODEL_PROJECT_ROOT"
  mkdir -p "$project_root"
else
  project_root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-real-model-smoke.XXXXXX")"
  owned_project="$project_root"
fi
cleanup() {
  if [[ -n "$owned_project" ]]; then
    rm -rf "$owned_project"
  fi
}
trap cleanup EXIT

run_rpotato() {
  RPOTATO_PROJECT_ROOT="$project_root" "$binary" "$@"
}

status="$(run_rpotato backend status)" || fail "backend status command failed"
grep -F -- '- status: running' <<<"$status" >/dev/null \
  || fail "managed backend is not running"
grep -F -- '- health: healthy' <<<"$status" >/dev/null \
  || fail "managed backend is not healthy"

classification="$(run_rpotato intent classify '안녕')" \
  || fail "conversation classification failed"
grep -F -- '- selected skill: conversation' <<<"$classification" >/dev/null \
  || fail "greeting did not route to conversation"

report="$(run_rpotato run '안녕. 반드시 한국어 한 문장으로만 답해줘.')" \
  || fail "real model conversation failed"
for expected in \
  '- 상태: 완료' \
  '- 선택한 skill: conversation' \
  '- action kind: answer-only' \
  '- 답변:'
do
  grep -F -- "$expected" <<<"$report" >/dev/null \
    || fail "completion report is missing: $expected"
done
answer="$(sed -n '/^- 답변:$/,$p' <<<"$report" | tail -n +2)"
[[ -n "${answer//[[:space:]]/}" ]] || fail "final answer is empty"
grep -Eq '[가-힣]' <<<"$answer" || fail "final answer has no Korean text"
grep -F 'MODEL ACTION' <<<"$report" >/dev/null \
  && fail "internal model action leaked into the final report"

printf '%s\n' 'managed real-model smoke ok'
printf '%s\n' "$report"
