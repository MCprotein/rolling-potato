#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

fail() {
  printf 'local tool schema contract failed: %s\n' "$1" >&2
  exit 1
}

require_literal() {
  local file="$1"
  local literal="$2"
  local label="$3"
  grep -F -- "$literal" "$file" >/dev/null || fail "$label"
}

require_text() {
  local body="$1"
  local literal="$2"
  local label="$3"
  grep -F -- "$literal" <<<"$body" >/dev/null || fail "$label"
}

agent=src/runtime_core/agent.rs
loop_state=src/app/tui_adapter/runtime/request/support/local_loop_state.rs
local_execution=src/app/tui_adapter/runtime/request/support/local_execution.rs
backend_chat=src/app/inference_adapter/backend/chat.rs
backend_tests=src/adapters/llama_cpp/backend/tests.rs
installer=src/adapters/llama_cpp/install.rs
workflow=.github/workflows/refactor-candidate.yml
preflight=scripts/ci/verify-pr-candidate-preflight.sh
managed_smoke=scripts/ci/verify-managed-real-model-smoke.sh
managed_smoke_test=scripts/ci/test-managed-real-model-smoke.sh

local_schema="$(grep -F 'pub(crate) const LOCAL_TURN_DECISION_JSON_SCHEMA' "$agent" || true)"
expected_local_schema='pub(crate) const LOCAL_TURN_DECISION_JSON_SCHEMA: &str = r#"{"type":"object","properties":{"decision":{"type":"string","enum":["answer","read_file","list_directory","search_repository","run_read_only_command","propose_mutation"]},"input":{"type":"string","maxLength":512},"answer":{"type":"string"}},"required":["decision","input","answer"],"additionalProperties":false}"#;'
[[ "$local_schema" == "$expected_local_schema" ]] || fail 'production local-turn schema drifted'
for decision in answer read_file list_directory search_repository run_read_only_command propose_mutation; do
  require_text "$local_schema" "\"$decision\"" "local schema decision is missing: $decision"
done
registry_contract="$(sed -n '/pub(crate) fn local_default()/,/^    }/p' "$agent")"
for tool in ReadFile ListDirectory SearchRepository RunReadOnlyCommand; do
  require_text "$registry_contract" "AgentToolId::$tool" "local registry tool is missing: $tool"
done
registry_tool_count="$(grep -c 'AgentToolId::' <<<"$registry_contract" | tr -d ' ')"
[[ "$registry_tool_count" == "4" ]] || fail "local registry must advertise exactly four tools, got $registry_tool_count"
observation_statuses="$(sed -n '/pub(crate) enum ToolObservationStatus/,/^}/p' "$agent")"
for status in Ok NotFound Denied ToolError Truncated Malformed UnknownOrStale Cancelled Timeout; do
  require_text "$observation_statuses" "$status" "observation status is missing: $status"
done
observation_status_count="$(grep -Ec '^    [A-Z][A-Za-z]+,$' <<<"$observation_statuses" | tr -d ' ')"
[[ "$observation_status_count" == "9" ]] || fail "observation status vocabulary drifted: $observation_status_count entries"

require_literal "$loop_state" 'pub(super) const MAX_MODEL_TURNS: u8 = 8;' 'model-turn budget drifted'
require_literal "$loop_state" 'pub(super) const MAX_TOOL_CALLS: u8 = 6;' 'tool-call budget drifted'
require_literal "$loop_state" 'pub(super) const TOOL_TIMEOUT: Duration = Duration::from_secs(5);' 'tool timeout drifted'
require_literal "$loop_state" 'pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);' 'request timeout drifted'
require_literal "$loop_state" 'pub(super) const MAX_OBSERVATION_BYTES: usize = 16 * 1024;' 'observation budget drifted'
require_literal "$loop_state" 'pub(super) const MAX_CUMULATIVE_OBSERVATION_BYTES: usize = 64 * 1024;' 'cumulative observation budget drifted'
for terminal in ModelTurnBudget ToolCallBudget RepeatedToolCall ProtocolError Cancelled ToolTimeout RequestDeadline ObservationBudget Answer ProposeMutation; do
  require_literal "$loop_state" "$terminal" "terminal state is missing: $terminal"
done
require_literal "$local_execution" 'LOCAL_TURN_DECISION_JSON_SCHEMA' 'production local execution is not wired to the local schema'
require_literal "$local_execution" 'remaining_request_time(started.elapsed())' 'production local execution does not consume one request-wide deadline'
require_literal "$local_execution" 'generate_structured_candidate_for_user_with_cancel_bounded' 'production local model turns are not bounded by the remaining request deadline'
require_literal "$local_execution" 'state.tool_timeout().min(remaining)' 'production local tools are not bounded by the remaining request deadline'
require_literal "$backend_chat" 'chat_once_with_input_for_intent_and_cancel_bounded' 'bounded structured backend entry point is missing'
require_literal "$backend_chat" 'Some(timeout_ms)' 'bounded structured backend timeout is not forwarded to the transport deadline'

require_literal "$installer" 'release_tag: "b9982"' 'managed llama.cpp revision drifted'
require_literal "$backend_tests" 'fn managed_llama_parser_accepts_local_turn_schema()' 'ignored live parser probe is missing'
require_literal "$backend_tests" '#[ignore = "requires the pinned managed llama.cpp server and checksummed tiny model"]' 'live parser probe must remain explicitly ignored outside its CI job'
require_literal "$backend_tests" 'LOCAL_TURN_DECISION_JSON_SCHEMA' 'live parser probe is not wired to the local schema'
require_literal "$backend_tests" 'chat_request_body_for_input(&input, 1,' 'live parser probe max_tokens must remain 1'
require_literal "$backend_tests" 'POST /v1/chat/completions HTTP/1.1' 'live parser endpoint drifted'
if sed -n '/fn managed_llama_parser_accepts_local_turn_schema()/,/^}/p' "$backend_tests" | grep -F 'TURN_DECISION_JSON_SCHEMA' | grep -Fv 'LOCAL_TURN_DECISION_JSON_SCHEMA' >/dev/null; then
  fail 'live parser probe references the legacy turn schema'
fi

parser_job="$(sed -n '/^  llama-parser-contract:/,/^  windows-compile:/p' "$workflow")"
require_text "$parser_job" 'llama-parser-contract:' 'required llama parser job is missing'
require_text "$parser_job" 'runs-on: ubuntu-24.04' 'llama parser runner drifted'
require_text "$parser_job" 'CANDIDATE_SHA: ${{ github.event.pull_request.head.sha }}' 'parser job candidate SHA pin is missing'
require_text "$parser_job" 'ref: ${{ github.event.pull_request.head.sha }}' 'parser checkout is not pinned to the candidate SHA'
require_text "$parser_job" 'https://huggingface.co/ggml-org/models/resolve/main/tinyllamas/stories260K.gguf' 'tiny parser model URL drifted'
require_text "$parser_job" '1185376' 'tiny parser model size drifted'
require_text "$parser_job" '270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d' 'tiny parser model checksum drifted'
require_text "$parser_job" 'x-linked-etag:' 'tiny parser model linked-etag verification is missing'
require_text "$parser_job" 'backend install' 'parser job does not use the managed installer'
require_text "$parser_job" 'backend start --model "$PARSER_MODEL" --ctx-size 512' 'parser server context size drifted'
require_text "$parser_job" 'managed_llama_parser_accepts_local_turn_schema' 'workflow is not wired to the live Rust parser probe'
require_literal "$workflow" 'scripts/ci/verify-local-tool-schema-contract.sh' 'candidate workflow is not wired to the static contract'
require_literal "$preflight" 'scripts/ci/verify-local-tool-schema-contract.sh' 'candidate preflight is not wired to the static contract'
require_literal "$managed_smoke" 'scenario="${2:-conversation}"' 'managed smoke scenario selector is missing'
require_literal "$managed_smoke" 'managed real-model local-tool smoke ok' 'managed local-tool behavior smoke is missing'
require_literal "$managed_smoke" 'smoke_data="$(mktemp -d' 'managed local-tool smoke does not isolate its data home'
require_literal "$managed_smoke" 'local-tool TUI prompt did not become ready' 'managed local-tool smoke does not wait for prompt readiness'
require_literal "$managed_smoke" 'wait_bounded "$tui_pid" 10' 'managed local-tool smoke lacks a bounded PTY shutdown'
require_literal "$managed_smoke" 'expected at least two persisted local tool observations' 'managed smoke does not require two observation rounds'
require_literal "$managed_smoke" 'expected two distinct local tools' 'managed smoke does not require distinct tool rounds'
require_literal "$managed_smoke_test" 'bad-local-tool-driver' 'managed local-tool smoke failure harness is missing'

printf '%s\n' 'local tool schema contract ok'
