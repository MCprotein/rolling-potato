# Observability And Monitoring

`rolling-potato` must directly observe per-model token usage, latency, memory, retries, guard results, tool results, and evidence results through the runtime core.

Monitoring is not external telemetry. It is a local-first runtime capability. The default is local storage, and user code or raw prompts are not sent outside the machine.

Monitoring UX follows [DESIGN.md](../DESIGN.md) and [tui.md](tui.md). TUI is the primary monitoring surface for SSH/Linux-server use; HTML is a later optional local report/dashboard surface.

## Goals

- Record per-model token and context usage.
- Compare backend/model performance and failure rates by session and workflow.
- Quantify small-model failure modes: invalid diffs, Korean guard rejections, tool failures, and stop-gate failures.
- Compare benchmark results and real-use results through the same schema.
- Let TUI and `doctor` show current state and recent failure causes.
- Support useful diagnosis without storing raw prompts or raw source code by default.

## Storage Decision

Default direction: SQLite plus append-only ledger.

- SQLite: query/index/reporting store
- append-only ledger: source runtime events and audit trail
- JSONL export: human-readable and issue-friendly export format

SQLite is the default local store because:

- model/session/workflow/tool/evidence data can be joined
- queries such as average tokens/sec by model, Korean guard rejection rate, and context truncation count are simple
- TUI can tail long sessions while showing aggregate screens quickly
- benchmark and real-use metrics can use the same query layer
- a single file is simple for low-end local distribution and backup

SQLite does not own:

- user approval policy
- stop-gate decisions
- source-of-truth event append order
- long-term raw prompt/source storage

## Current Implementation

Phase 2 currently implements the runtime store foundation.

- `rpotato init` creates app data root, project-local `.rpotato/`, current state, runtime ledger, project session ledger, runtime evidence JSONL, and SQLite projection.
- Append-only ledger is the source of truth; SQLite `ledger_events` is a replayable projection.
- SQLite session history can be restored for the current project from replayed `ledger_events` if the projection is recreated.
- SQLite migration v1 creates `sessions`, `workflows`, `workflow_transitions`, `checkpoint_records`, `model_runs`, `token_usage`, `backend_runs`, `tool_calls`, `command_runs`, `guard_results`, `stop_gate_results`, `evidence_records`, and `benchmark_runs`.
- `rpotato state` shows current-state and ledger/projection counts.
- `rpotato state reconcile` recovers missing/stale/corrupt current state and records preserve-move events in the ledger.
- `rpotato state resume` distinguishes no active workflow, active pointer detected, and blocked states, then records a ledger event.
- `rpotato session list` and `rpotato session history` read session history from the SQLite projection for the current project.
- `rpotato session new` creates a fresh session identity, writes it to current state, appends a `session.new` ledger event, and projects it into SQLite.
- `rpotato session resume <session-id>` and `rpotato resume <session-id>` select a prior session from SQLite history and write that session id back into current state.
- `rpotato resume` without an id shows session history, so a TUI/CLI surface can let users choose the target before resuming.
- `rpotato cancel` appends a no-op cancel event when there is no active workflow.
- `rpotato evidence validate <artifact-pointer>` verifies that a project-relative artifact pointer stays inside the project boundary.
- `rpotato monitor status` and `rpotato monitor models` read SQLite projection.
- `rpotato monitor export --format jsonl|csv` renders runtime ledger/projection into human-readable exports.
- `rpotato monitor prune --before 30d --dry-run` calculates only deletion candidate counts.
- A corrupt SQLite file is preserved with a `.corrupt.<timestamp>` suffix before a new projection is created.
- Corrupt/stale current state is preserved by `state reconcile` with `.corrupt.<timestamp>` or `.stale.<timestamp>` suffixes.
- Evidence is stale when the artifact is missing, escapes the project boundary, or exceeds `stale_after_ms`.

Not implemented yet:

- token/latency/resource metric recording from real model/backend execution
- full transcript replay and conversation continuation after a selected session resume
- active workflow resume execution by the real agent loop
- actual retention deletion

## Local File Layout

Expected locations:

```text
rpotato app data root/
  state/
    runtime-ledger.jsonl
    observability.sqlite
    observability.sqlite-wal
  logs/
    backend/
    commands/

project root/
  .rpotato/
    evidence/
    session-ledger.jsonl
```

Project-local ledgers are closer to project boundary and evidence. App-level SQLite is closer to cross-project model/runtime monitoring.

## Required Metrics

### Model Run

- model id
- model artifact hash
- backend id and version
- quantization
- context length limit
- prompt tokens
- completion tokens
- total tokens
- context tokens used
- context tokens dropped
- ontology tokens
- tool summary tokens
- max output tokens
- first token latency
- total latency
- prompt eval time
- generation eval time
- tokens per second
- cancellation flag

### Runtime Resource

- backend startup time
- process uptime
- peak RSS memory
- disk space used by models/cache/logs
- backend crash count
- health check latency
- active session count

### Agent Reliability

- workflow id
- active skill id
- subagent/team id
- retry count
- regeneration count
- invalid action count
- invalid diff count
- tool failure count
- command exit-code class
- Korean guard pass/fail
- stop gate pass/fail
- missing evidence count

### Privacy And Safety

- approval prompt count
- denied action count
- destructive command blocked count
- credential redaction count
- project-boundary violation count
- network download approval count

## Schema Direction

Initial SQLite table candidates:

```text
schema_migrations
sessions
workflows
ledger_events
model_runs
token_usage
backend_runs
tool_calls
command_runs
guard_results
stop_gate_results
evidence_records
benchmark_runs
```

Principles:

- Do not store raw prompts or raw source by default.
- Store source paths as project-relative paths plus hashes.
- Prefer redacted summaries and artifact pointers for command output.
- Keep raw logs opt-in or short-retention.
- Schema migrations are versioned and forward-only.

## CLI/TUI Surface

Initial commands:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor session <id>
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
```

TUI views:

- model/token usage summary
- live session latency and token-stream stats
- backend health
- guard/stop-gate results
- subagent/team metric summary
- recent failures and validation gaps

HTML is not the MVP primary surface. If added later, it should be a local-only report or dashboard reading SQLite/export data. HTML must not become a separate monitoring source of truth.

## Retention

Retention balances privacy and debugging value.

Initial principles:

- aggregate metrics may be kept long term
- raw command output and backend logs use short retention
- credential-like values are redacted before persistence
- exports scan for sensitive information before writing
- `rpotato monitor prune` supports dry-run

## Compaction And Resume Policy

Compacted summaries are not source of truth.

- Current state stores only `compaction_boundary` and `compacted_summary_path` pointers.
- Original decision evidence is rechecked from runtime ledger, project session ledger, and evidence artifact pointers.
- Compacted summary is only a resume-bundle navigation hint and is not used to confirm file, command, or model claims.
- Compacted summary artifacts must pass the same project-boundary validation as `evidence validate`.
- Active workflow resume detects current-state pointers, records ledger events, and leaves actual execution to the later agent-loop phase.
- Session resume is history-first: SQLite provides the selectable session list, append-only ledger remains the audit source, and current state stores only the selected `session_id` plus resume metadata.
- `rpotato resume <session-id>` currently selects the target session for subsequent commands; model transcript replay is a later agent-loop capability.

## Validation

Required tests:

- SQLite schema migration
- SQLite projection after event ledger append
- token usage aggregation
- per-model metric query
- raw prompt/source not stored by default
- redaction before persistence
- corrupt SQLite fallback
- JSONL export
- retention prune dry-run
- TUI metric view smoke test
