#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="${1:-$repo_root/target/debug/rpotato}"
scenario="${2:-conversation}"

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
local_project=""
smoke_data=""
local_backend_started=0
active_tui_pid=""
if [[ -n "${RPOTATO_REAL_MODEL_PROJECT_ROOT:-}" ]]; then
  project_root="$RPOTATO_REAL_MODEL_PROJECT_ROOT"
  mkdir -p "$project_root"
else
  project_root="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-real-model-smoke.XXXXXX")"
  owned_project="$project_root"
fi
cleanup() {
  if [[ -n "$active_tui_pid" ]]; then
    kill "$active_tui_pid" >/dev/null 2>&1 || true
    wait "$active_tui_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$local_backend_started" == "1" ]]; then
    RPOTATO_PROJECT_ROOT="$local_project" \
    RPOTATO_DATA_HOME="$smoke_data" \
    RPOTATO_BACKEND_LLAMA_CPP_PATH="${source_binary:-}" \
    RPOTATO_BACKEND_PORT="${smoke_port:-}" \
      "$binary" backend stop >/dev/null 2>&1 || true
  fi
  if [[ -n "$local_project" ]]; then
    rm -rf "$local_project"
  fi
  if [[ -n "$smoke_data" ]]; then
    rm -rf "$smoke_data"
  fi
  if [[ -n "$owned_project" ]]; then
    rm -rf "$owned_project"
  fi
}
trap cleanup EXIT

wait_bounded() {
  local pid="$1"
  local seconds="$2"
  for _ in $(seq 1 "$seconds"); do
    if ! jobs -pr | grep -qx "$pid"; then
      wait "$pid"
      return $?
    fi
    sleep 1
  done
  kill "$pid" >/dev/null 2>&1 || true
  sleep 1
  kill -9 "$pid" >/dev/null 2>&1 || true
  wait "$pid" >/dev/null 2>&1 || true
  return 124
}

run_rpotato() {
  RPOTATO_PROJECT_ROOT="$project_root" "$binary" "$@"
}

status="$(run_rpotato backend status)" || fail "backend status command failed"
grep -F -- '- status: running' <<<"$status" >/dev/null \
  || fail "managed backend is not running"
grep -F -- '- health: healthy' <<<"$status" >/dev/null \
  || fail "managed backend is not healthy"

if [[ "$scenario" == "local-tool" ]]; then
  report_field() {
    local label="$1"
    sed -n "s/^- $label: //p" <<<"$status" | head -n 1
  }
  source_binary="$(report_field binary)"
  source_model="$(report_field model)"
  source_ctx="$(report_field 'ctx size')"
  [[ -x "$source_binary" ]] || fail "running backend binary is unavailable: $source_binary"
  [[ -f "$source_model" ]] || fail "running supported model is unavailable: $source_model"
  [[ "$source_ctx" =~ ^[1-9][0-9]*$ ]] || fail "running backend context size is invalid: $source_ctx"

  local_project="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-local-tool-project.XXXXXX")"
  smoke_data="$(mktemp -d "${TMPDIR:-/tmp}/rpotato-local-tool-data.XXXXXX")"
  smoke_port="${RPOTATO_REAL_MODEL_SMOKE_PORT:-$((20000 + $$ % 20000))}"
  export RPOTATO_DATA_HOME="$smoke_data"
  export RPOTATO_PROJECT_ROOT="$local_project"
  export RPOTATO_BACKEND_LLAMA_CPP_PATH="$source_binary"
  export RPOTATO_BACKEND_PORT="$smoke_port"
  project_root="$local_project"
  run_rpotato init >/dev/null || fail "temporary local-tool project initialization failed"
  run_rpotato backend start --model "$source_model" --ctx-size "$source_ctx" >/dev/null \
    || fail "smoke-owned backend start failed"
  local_backend_started=1
  printf '%s\n' 'LOCAL_TOOL_SMOKE_README_FACT' >"$project_root/README.md"
  printf '%s\n' '[package]' 'name = "local-tool-smoke"' >"$project_root/Cargo.toml"
  capture="$project_root/local-tool-smoke.terminal"
  transcript_root="$smoke_data/state/transcripts"
  request='README.md를 먼저 읽고 Cargo.toml에서 package name도 확인한 뒤, 반드시 서로 다른 로컬 도구를 두 번 사용하고 확인한 두 사실을 한국어 한 문장으로 답해줘.'

  if [[ -n "${RPOTATO_REAL_MODEL_TUI_DRIVER:-}" ]]; then
    "$RPOTATO_REAL_MODEL_TUI_DRIVER" "$binary" "$project_root" "$capture" "$request" &
    active_tui_pid=$!
    wait_bounded "$active_tui_pid" 120 || fail "local-tool TUI driver failed or timed out"
    active_tui_pid=""
  else
    command -v script >/dev/null || fail "PTY command 'script' is required for local-tool smoke"
    fifo="$project_root/local-tool-smoke.stdin"
    mkfifo "$fifo"
    exec 3<>"$fifo"
    if [[ "$(uname -s)" == "Darwin" ]]; then
      env RPOTATO_PROJECT_ROOT="$project_root" script -q "$capture" "$binary" tui interactive <&3 &
    else
      quoted_binary="${binary//\'/\'\\\'\'}"
      env RPOTATO_PROJECT_ROOT="$project_root" script --quiet --return --command "'$quoted_binary' tui interactive" "$capture" <&3 &
    fi
    tui_pid=$!
    active_tui_pid="$tui_pid"
    ready=0
    for _ in $(seq 1 20); do
      if grep -F 'local ready' "$capture" >/dev/null 2>&1; then
        ready=1
        break
      fi
      kill -0 "$tui_pid" 2>/dev/null || break
      sleep 1
    done
    if [[ "$ready" != "1" ]]; then
      kill "$tui_pid" >/dev/null 2>&1 || true
      wait "$tui_pid" >/dev/null 2>&1 || true
      active_tui_pid=""
      fail "local-tool TUI prompt did not become ready"
    fi
    printf '%s\n' "$request" >&3
    completed=0
    for _ in $(seq 1 90); do
      records="$(find "$transcript_root" -type f -name '*.json' -print0 2>/dev/null | xargs -0 grep -l '"kind":"model"' 2>/dev/null || true)"
      if [[ -n "$records" ]]; then
        completed=1
        break
      fi
      kill -0 "$tui_pid" 2>/dev/null || break
      sleep 1
    done
    printf '%s\n' '/quit' >&3 || true
    exec 3>&-
    wait_bounded "$tui_pid" 10 || fail "local-tool TUI exit failed or timed out"
    active_tui_pid=""
    [[ "$completed" == "1" ]] || fail "local-tool TUI did not produce a visible answer before timeout"
  fi

  transcript_dump="$(find "$transcript_root" -type f -name '*.json' -exec cat {} + 2>/dev/null || true)"
  final_answers="$(grep -F '"kind":"model"' <<<"$transcript_dump" || true)"
  [[ -n "$final_answers" ]] || fail "local-tool final visible answer transcript is missing"
  grep -Eq '[가-힣]' <<<"$final_answers" \
    || fail "local-tool final visible answer has no Korean text"
  tool_records="$(grep -o '\\"event_type\\":\\"tool_activity\\"' <<<"$transcript_dump" | wc -l | tr -d ' ' || true)"
  [[ "$tool_records" -ge 2 ]] || fail "expected at least two persisted local tool observations, got $tool_records"
  distinct_tools="$(grep -o '\\"tool\\":\\"\(read_file\|list_directory\|search_repository\|run_read_only_command\)\\"' <<<"$transcript_dump" | sort -u | wc -l | tr -d ' ' || true)"
  [[ "$distinct_tools" -ge 2 ]] || fail "expected two distinct local tools, got $distinct_tools"
  printf '%s\n' 'managed real-model local-tool smoke ok'
  exit 0
fi

[[ "$scenario" == "conversation" ]] || fail "unknown scenario: $scenario"

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
