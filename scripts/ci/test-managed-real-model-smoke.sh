#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
smoke="$repo_root/scripts/ci/verify-managed-real-model-smoke.sh"
root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-real-model-smoke-test.XXXXXX")"
trap 'rm -rf "$root"' EXIT
fake="$root/rpotato"

cat >"$fake" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  'backend status')
    printf '%s\n' 'backend status' '- status: running' '- health: healthy'
    ;;
  'intent classify 안녕')
    printf '%s\n' 'intent classify 결과' '- selected skill: conversation'
    ;;
  'run 안녕. 반드시 한국어 한 문장으로만 답해줘.')
    printf '%s\n' \
      'run 결과' \
      '- 상태: 완료' \
      '- 선택한 skill: conversation' \
      '- action kind: answer-only' \
      '- 답변:' \
      '안녕하세요. 무엇을 도와드릴까요?'
    ;;
  *)
    exit 64
    ;;
esac
SH
chmod +x "$fake"

if "$smoke" "$fake" >"$root/not-opted-in.out" 2>&1; then
  printf '%s\n' 'real-model smoke unexpectedly ran without explicit opt-in' >&2
  exit 1
else
  status=$?
fi
[[ "$status" -eq 2 ]] || {
  printf 'expected opt-in refusal exit 2, got %s\n' "$status" >&2
  exit 1
}

output="$(
  RPOTATO_REAL_MODEL_SMOKE=1 \
  RPOTATO_REAL_MODEL_PROJECT_ROOT="$root/project" \
    "$smoke" "$fake"
)"
grep -F 'managed real-model smoke ok' <<<"$output" >/dev/null
grep -F -- '- action kind: answer-only' <<<"$output" >/dev/null
grep -F '안녕하세요.' <<<"$output" >/dev/null

printf '%s\n' 'managed real-model smoke contract ok'
