# Observability And Monitoring

`rolling-potato` must directly observe per-model token usage, latency, CPU, memory, retries, guard results, tool results, and evidence results through the runtime core.

Monitoring is not external telemetry. It is a local-first runtime capability. The default is local storage, and user code or raw prompts are not sent outside the machine.

Monitoring UX follows [DESIGN.md](../DESIGN.md) and [tui.md](tui.md). TUI is the primary monitoring surface for SSH/Linux-server use; HTML is a later optional local report/dashboard surface.

## Goals

- Record per-model token and context usage.
- Record backend sidecar CPU, memory, disk, and resource-pressure samples.
- Compare backend/model performance and failure rates by session and workflow.
- Quantify small-model failure modes: invalid diffs, Korean guard rejections, tool failures, and stop-gate failures.
- Compare benchmark results and real-use results through the same schema.
- Let TUI and `doctor` show current state and recent failure causes.
- Let the runtime reduce work when local resources are under pressure instead of waiting for the OS to fail first.
- Support useful diagnosis without storing raw prompts or raw source code by default.

## Resource Monitoring Rollout

Resource monitoring must land before any autonomous resource governor consumes
it. The release grouping is:

| Version | Scope | Contract |
| --- | --- | --- |
| v0.9.0 | resource sampler and logging | sample backend sidecar CPU, average/peak RSS, disk/cache/log bytes, sample count, and pressure status; write redacted ledger events and SQLite projection rows |
| v0.10.0 | TUI monitor display | show CPU, memory, latency, token throughput, and pressure state in terminal-safe layouts |
| v0.11.0 | backend chat resource governor | sample before chat, block critical pressure, clamp degraded-pressure max tokens, and report governor decisions in CLI/runtime ledger output |
| v0.12.0 | team admission preview | read the latest resource sample, report admitted lanes, prefer sequential fallback on unknown/degraded pressure, and block dispatch on critical pressure |
| v0.13.0 | team admission gate | enforce requested lane admission before dispatch, record the decision in the ledger, fall back to one sequential lane on unknown/degraded pressure, and block critical pressure |
| v0.14.0 | team admission policy preflight | run requested write path and command policy checks before dispatch; allow-only checks pass, ask/deny checks block worker launch |
| v0.15.0 | team file ownership preflight | normalize lane-owned write paths before dispatch, record ownership status in ledger output, and block cross-lane write conflicts |
| v0.16.0 | team admission approval queue integration | write project-local approval request records for blocked policy/ownership decisions and render them in `tui approvals` |
| v0.17.0 | context/model governor preflight | clamp requested context against the configured budget and resource pressure, emit model route hints, and record the decision in the ledger |
| v0.18.0+ | remaining dispatcher governor policy | add dispatch-time ownership enforcement and failed-worker continuation |

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
- SQLite migration v2 creates `sessions`, `workflows`, `workflow_transitions`, `checkpoint_records`, `resource_samples`, `model_runs`, `token_usage`, `backend_runs`, `tool_calls`, `command_runs`, `guard_results`, `stop_gate_results`, `evidence_records`, and `benchmark_runs`.
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
- `rpotato backend start`, `rpotato backend status`, and `rpotato backend chat` record event-driven backend CPU/RSS/disk resource samples.
- `rpotato backend chat` applies the first runtime resource governor slice: critical pressure blocks chat before model execution, degraded pressure clamps the effective max-token budget, and normal/unknown pressure preserves the requested token budget.
- `rpotato team status` reads the latest resource sample and reports read-only team admission: normal pressure admits parallel lanes, unknown/degraded pressure falls back to one sequential lane, and critical pressure blocks dispatch.
- `rpotato team admit --lanes <count>` is the first enforced team admission gate. It records the admission decision in the append-only ledger and SQLite projection, admits requested lanes on normal pressure, falls back to one sequential lane on unknown/degraded pressure, and returns a blocked error on critical pressure before any worker launch exists.
- `rpotato team admit --lanes <count> --write <path> --command <command>` adds policy preflight to the admission gate. Requested write paths and commands are classified with the same policy engine used by `policy check-path` and `policy check-command`; any `ask` or `deny` decision blocks dispatch and is recorded in the team admission ledger event.
- `rpotato team admit --lanes <count> --write-owner <lane:path>` adds file ownership preflight. Ownership paths are normalized before dispatch; the same normalized write path cannot be owned by multiple lanes, and conflicts are recorded as blocked team admission events.
- Blocked team admission policy/ownership decisions write redacted project-local approval request records under `.rpotato/approval-requests/`, and `rpotato tui approvals` lists those records alongside patch proposal approvals.
- `rpotato team governor --lanes <count> --context-tokens <tokens>` records a context/model governor preflight. It consumes the latest resource sample, reports admitted lanes, clamps effective context tokens against `--context-limit` or the runtime default, emits local model-tier route hints (`keep`, `downgrade`, `escalate`, `defer`), and records the decision in the append-only ledger and SQLite projection. These hints are local runtime policy hints, not source-backed claims about a real model artifact.
- A corrupt SQLite file is preserved with a `.corrupt.<timestamp>` suffix before a new projection is created.
- Corrupt/stale current state is preserved by `state reconcile` with `.corrupt.<timestamp>` or `.stale.<timestamp>` suffixes.
- Evidence is stale when the artifact is missing, escapes the project boundary, or exceeds `stale_after_ms`.

Not implemented yet:

- continuous background CPU/memory/disk resource sampling from the managed backend sidecar
- full subagent/team dispatcher execution after admission
- dispatch-time ownership enforcement
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
- process CPU percent
- average RSS memory
- peak RSS memory
- resource sample count
- resource pressure status: normal, degraded, critical
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
resource_samples
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
- CPU/memory/resource-pressure summary
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

Initial retention matrix:

| Data | Default retention | Delete/prune surface | Notes |
| --- | --- | --- | --- |
| aggregate metrics | long term | `monitor prune --dry-run` then explicit prune | no raw prompt/source |
| SQLite projection | long term while app data exists | rebuildable from ledger where possible | projection, not event source |
| append-only runtime ledger | long term | explicit user cleanup only | audit source |
| project session ledger | project-local | project cleanup only | tied to `.rpotato/` |
| transcript metadata | project-local | project cleanup only | raw transcript storage remains opt-in/later |
| evidence artifacts | until stale or user cleanup | `evidence validate`, later evidence prune | project-bound pointer required |
| command output summaries | short or redacted | monitor/log prune | prefer summaries over raw logs |
| backend logs | short | monitor/log prune | useful for crashes, privacy-sensitive |
| benchmark reports | long term if redacted | benchmark/report prune | include reproducibility manifest |
| model knowledge entries | long term if redacted | `model knowledge prune --dry-run` | store pointers, not raw prompts/source |
| plugin data | until plugin removal | `plugin remove --keep-data|--purge-data` | plugin data is separated from plugin source |
| JSONL/CSV exports | user-owned artifact | user deletion | export command scans before writing |

Export redaction behavior:

- exports fail closed when a sensitive value cannot be safely redacted
- failed exports should record a ledger event with a redacted reason
- export artifacts are never treated as a new source of truth
- exports must preserve enough ids to re-query local evidence without storing
  raw prompt or source text

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
- resource sample projection
- resource pressure classification boundary tests
- raw prompt/source not stored by default
- redaction before persistence
- corrupt SQLite fallback
- JSONL export
- retention prune dry-run
- TUI metric view smoke test
