#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
smoke="$repo_root/scripts/ci/verify-managed-real-model-smoke.sh"
root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-real-model-smoke-test.XXXXXX")"
trap 'rm -rf "$root"' EXIT
fake="$root/rpotato"
driver="$root/local-tool-driver"

cat >"$fake" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  'backend status')
    printf '%s\n' \
      'backend status' \
      '- status: running' \
      "- binary: $0" \
      "- model: ${RPOTATO_REAL_MODEL_FAKE_MODEL:?}" \
      '- ctx size: 4096' \
      '- health: healthy'
    ;;
  backend\ start\ --model\ *\ --ctx-size\ 4096)
    mkdir -p "${RPOTATO_DATA_HOME:?}/state"
    ;;
  'backend stop')
    ;;
  'init')
    mkdir -p "${RPOTATO_PROJECT_ROOT:?}/.rpotato"
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

cat >"$driver" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
binary="$1"
project_root="$2"
capture="$3"
request="$4"
[[ -x "$binary" ]]
[[ -f "$project_root/README.md" ]]
[[ -f "$project_root/Cargo.toml" ]]
[[ "$request" == *'서로 다른 로컬 도구를 두 번'* ]]
transcripts="${RPOTATO_DATA_HOME:?}/state/transcripts/project/session"
mkdir -p "$transcripts"
printf '%s' '{"kind":"evidence","content":"{\"event_type\":\"tool_activity\",\"tool\":\"read_file\"}"}' >"$transcripts/tool-1.json"
printf '%s' '{"kind":"evidence","content":"{\"event_type\":\"tool_activity\",\"tool\":\"search_repository\"}"}' >"$transcripts/tool-2.json"
printf '%s' '{"kind":"model","content":"두 파일을 확인했습니다."}' >"$transcripts/model.json"
printf '%s\n' '두 파일을 확인했습니다.' >"$capture"
SH
chmod +x "$driver"
fake_model="$root/model.gguf"
printf '%s\n' 'fake supported model' >"$fake_model"
export RPOTATO_REAL_MODEL_FAKE_MODEL="$fake_model"

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

local_output="$(
  RPOTATO_REAL_MODEL_SMOKE=1 \
  RPOTATO_REAL_MODEL_PROJECT_ROOT="$root/local-project" \
  RPOTATO_DATA_HOME="$root/local-data" \
  RPOTATO_REAL_MODEL_TUI_DRIVER="$driver" \
    "$smoke" "$fake" local-tool
)"
grep -F 'managed real-model local-tool smoke ok' <<<"$local_output" >/dev/null

bad_driver="$root/bad-local-tool-driver"
sed 's/search_repository/read_file/' "$driver" >"$bad_driver"
chmod +x "$bad_driver"
if RPOTATO_REAL_MODEL_SMOKE=1 \
  RPOTATO_REAL_MODEL_PROJECT_ROOT="$root/bad-local-project" \
  RPOTATO_DATA_HOME="$root/bad-local-data" \
  RPOTATO_REAL_MODEL_TUI_DRIVER="$bad_driver" \
    "$smoke" "$fake" local-tool >"$root/bad-local.out" 2>&1; then
  printf '%s\n' 'local-tool smoke accepted repeated identical tools' >&2
  exit 1
fi
grep -F 'expected two distinct local tools' "$root/bad-local.out" >/dev/null

no_answer_driver="$root/no-answer-local-tool-driver"
grep -Fv '"kind":"model"' "$driver" >"$no_answer_driver"
chmod +x "$no_answer_driver"
if RPOTATO_REAL_MODEL_SMOKE=1 \
  RPOTATO_REAL_MODEL_PROJECT_ROOT="$root/no-answer-local-project" \
  RPOTATO_DATA_HOME="$root/no-answer-local-data" \
  RPOTATO_REAL_MODEL_TUI_DRIVER="$no_answer_driver" \
    "$smoke" "$fake" local-tool >"$root/no-answer-local.out" 2>&1; then
  printf '%s\n' 'local-tool smoke accepted a missing final answer transcript' >&2
  exit 1
fi
grep -F 'local-tool final visible answer transcript is missing' "$root/no-answer-local.out" >/dev/null

printf '%s\n' 'managed real-model smoke contract ok'
