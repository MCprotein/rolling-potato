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
- Support useful diagnosis without storing the complete backend prompt, hidden/raw model response, or raw source body.

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
| v0.18.0 | performance baseline report | aggregate local p50/p95 latency, tokens/sec, context clamp count, peak RSS, pressure state, and backend/model/session metrics without storing raw prompt/source text |
| v0.19.0 | benchmark harness foundation | record benchmark runs in the ledger/projection, validate fixture metadata, emit reproducibility metadata, and export redacted local reports |
| v0.20.0 | executable benchmark runner | link active-backend prompt artifact runs, local score, token/latency, and resource metrics through the same runtime monitoring schema |
| v0.21.0 | benchmark-driven optimization policy | `monitor optimize` recommends context budget, lane count, fallback, and model route from measured local metrics and benchmark evidence |
| v0.22.0 | dispatcher hardening | enforce dispatch-time file ownership, record failed-worker continuation, and surface latest team runtime status |

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
- long-term complete backend-prompt, hidden-response, or raw-source storage

## Current Implementation

Phase 2 currently implements the runtime store foundation.

- `rpotato init` creates app data root, project-local `.rpotato/`, current state, runtime ledger, project session ledger, runtime evidence JSONL, and SQLite projection.
- Append-only ledger is the source of truth; SQLite `ledger_events` is a replayable projection.
- SQLite session history can be restored for the current project from replayed `ledger_events` if the projection is recreated.
- SQLite migration v6 adds rebuildable `transcript_records` and continues rebuilding `workflows` from append-only `workflow.checkpoint` details; SQLite remains a projection, not workflow or transcript authority.
- `rpotato state` shows current-state and ledger/projection counts.
- `rpotato state reconcile` recovers missing/stale/corrupt current state and records preserve-move events in the ledger.
- `rpotato state resume` strictly parses every ledger line, validates the full canonical snapshot/checkpoint hash chain and latest committed revision, and resumes safe phases idempotently. Pending approval displays the diff and a token placeholder without a backend call; the one-time token cannot be redisplayed.
- Patch verification writes project evidence JSON plus runtime evidence JSONL containing hashes and status, not source, command output, or approval token plaintext. The stop gate rereads the artifact and authoritative source before success.
- `rpotato session list` and `rpotato session history` read a SQLite session view rebuilt from the canonical runtime ledger for the current project. Replay removes SQLite-only session rows.
- `rpotato session new` creates a fresh session identity, writes it to current state, appends a `session.new` ledger event, and projects it into SQLite.
- `rpotato session resume <session-id>`, `rpotato resume <session-id>`, and `rpotato continue <session-id>` require canonical ledger ownership, validate immutable transcript artifacts and source hashes before current-state mutation, and continue only a matching safe workflow checkpoint. Bare `continue` resumes the current selection.
- `rpotato resume` without an id shows session history, so a TUI/CLI surface can let users choose the target before resuming.
- `rpotato cancel` appends a no-op cancel event when there is no active workflow.
- `rpotato evidence validate <artifact-pointer>` verifies that a project-relative artifact pointer stays inside the project boundary.
- `rpotato monitor status` and `rpotato monitor models` read SQLite projection.
- `rpotato monitor baseline` aggregates local ledger/SQLite projection metrics into a read-only performance baseline report with p50/p95 latency, average tokens/sec, context clamp count, peak RSS, pressure-state distribution, and model/backend/session grouping. It does not store raw prompt/source text and does not choose model artifacts.
- `rpotato monitor optimize` reads the local performance baseline, latest resource sample, and `measured-locally` benchmark rows to recommend context budget, team lane count, fallback mode, and model route hint. It is read-only and does not select a real model artifact, promote model status, or claim public benchmark parity.
- `rpotato monitor export --format jsonl|csv` renders runtime ledger/projection into human-readable exports.
- `rpotato monitor prune --before 30d --dry-run` calculates only deletion candidate counts.
- `rpotato benchmark validate <fixture.json>` validates project-local fixture metadata for runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required tool/source/evidence records, abstention requirement, ontology view, context budget, backend/model artifact identifiers, sampling policy, and raw artifact retention policy.
- `rpotato benchmark record --fixture <fixture.json>` records a metadata-only benchmark run in the append-only ledger and SQLite `benchmark_runs` projection with `claim_state=not-comparable`, no score, a reproducibility manifest, and a redacted local report.
- `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>` executes the prompt artifact through the running backend sidecar and records a local `measured-locally` benchmark row linked to `model_run_id`, prompt artifact checksum, token/latency/resource summaries, deterministic score metadata, reproducibility manifest, and redacted report.
- `rpotato benchmark report --format jsonl` exports the redacted benchmark projection with reproducibility metadata. Public benchmark parity remains explicitly unclaimed.
- `rpotato backend start`, `rpotato backend status`, and `rpotato backend chat` record event-driven backend CPU/RSS/disk resource samples.
- `rpotato backend chat` applies the first runtime resource governor slice: critical pressure blocks chat before model execution, degraded pressure clamps the effective max-token budget, and normal/unknown pressure preserves the requested token budget.
- Backend chat records first-visible-token and total latency, completed token usage, effective output budget, terminal resource samples, and lifecycle events. Cancellation and timeout retain distinct ledger event types while the model-run interruption flag marks both as interrupted.
- If an interrupted or failed SSE stream does not deliver the final usage chunk, `token_usage` is intentionally omitted for that run. Missing usage is unknown, not zero. Raw prompt, raw response, and reasoning trace are not persisted.
- `rpotato team status` reads the latest resource sample and reports read-only team admission: normal pressure admits parallel lanes, unknown/degraded pressure falls back to one sequential lane, and critical pressure blocks dispatch. It also surfaces the latest `team.*` runtime ledger event for the current project.
- `rpotato team admit --lanes <count>` is the first enforced team admission gate. It records the admission decision in the append-only ledger and SQLite projection, admits requested lanes on normal pressure, falls back to one sequential lane on unknown/degraded pressure, and returns a blocked error on critical pressure before any worker launch exists.
- `rpotato team admit --lanes <count> --write <path> --command <command>` adds policy preflight to the admission gate. Requested write paths and commands are classified with the same policy engine used by `policy check-path` and `policy check-command`; any `ask` or `deny` decision blocks dispatch and is recorded in the team admission ledger event.
- `rpotato team admit --lanes <count> --write-owner <lane:path>` adds file ownership preflight. Ownership paths are normalized before dispatch; the same normalized write path cannot be owned by multiple lanes, and conflicts are recorded as blocked team admission events.
- Blocked team admission policy/ownership decisions write redacted project-local approval request records under `.rpotato/approval-requests/`, and `rpotato tui approvals` lists those records alongside patch proposal approvals.
- `rpotato team dispatch --lanes <count> --write-owner <lane:path>` rechecks normalized file ownership at the dispatch boundary, records ready/fallback/blocked events in the append-only ledger and SQLite projection, and blocks cross-lane ownership conflicts before worker launch exists. `--failed-lane <lane> --failure <reason>` records failed-worker continuation state and whether remaining admitted lanes may continue.
- `rpotato team governor --lanes <count> --context-tokens <tokens>` records a context/model governor preflight. It consumes the latest resource sample, reports admitted lanes, clamps effective context tokens against `--context-limit` or the runtime default, emits local model-tier route hints (`keep`, `downgrade`, `escalate`, `defer`), and records the decision in the append-only ledger and SQLite projection. These hints are local runtime policy hints, not source-backed claims about a real model artifact.
- A corrupt SQLite file is preserved with a `.corrupt.<timestamp>` suffix before a new projection is created.
- Corrupt/stale current state is preserved by `state reconcile` with `.corrupt.<timestamp>` or `.stale.<timestamp>` suffixes.
- Evidence is stale when the artifact is missing, escapes the project boundary, or exceeds `stale_after_ms`.

Not implemented yet:

- continuous background CPU/memory/disk resource sampling from the managed backend sidecar
- full subagent/team dispatcher execution after dispatch preflight
- compaction/summarization of transcript histories beyond the bounded recent-turn window
- actual retention deletion
- separate SQLite terminal-outcome enum for cancellation versus timeout
- live TUI rendering of token-stream statistics

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
transcript_records
```

Principles:

- Durable resume stores user and visible/normalized model/tool/evidence turns; normalized patch actions store paths, action metadata, and SHA-256 values instead of patch fragments or verification-command text.
- Do not store complete backend prompts, hidden/raw model responses, or complete source-file bodies in the transcript projection.
- Store source paths as project-relative paths plus hashes.
- Prefer redacted summaries and artifact pointers for command output.
- Keep raw logs opt-in or short-retention.
- Schema migrations are versioned and forward-only.

## CLI/TUI Surface

Initial commands:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor session <id>
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato continue
rpotato continue <session-id>
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
| durable transcript artifacts | app-data lifetime | app-data cleanup only | local user plus visible/normalized model/tool/evidence turns; no hidden response or raw source body |
| evidence artifacts | until stale or user cleanup | `evidence validate`, later evidence prune | project-bound pointer required |
| patch rollback bytes | until project cleanup | project `.rpotato/` cleanup | restricted project-local original bytes; never projected to SQLite/monitor or ledger/evidence payloads |
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

The SQLite projection includes verification-evidence and stop-gate result rows
for status/TUI queries. These rows contain IDs, hashes, pass/fail state, and event
timestamps only. They never contain the raw source retained by a rollback artifact.

## Compaction And Resume Policy

Compacted summaries are not source of truth.

- Current state stores only `compaction_boundary` and `compacted_summary_path` pointers.
- Original decision evidence is rechecked from runtime ledger, project session ledger, and evidence artifact pointers.
- Compacted summary is only a resume-bundle navigation hint and is not used to confirm file, command, or model claims.
- Compacted summary artifacts must pass the same project-boundary validation as `evidence validate`.
- Runtime core reconstructs up to 8 recent transcript turns/2,400 characters and applies one shared 4-pointer/3,200-character source budget across current-request and resumed context before creating or continuing a workflow.
- Session resume is ledger/artifact-authoritative: SQLite renders selectable session/transcript views, while append-only ledger events and immutable transcript artifacts authorize replay. Current state stores the selected `session_id` plus resume metadata.
- Every transcript projection row stores its canonical ledger event ID and monotonic event ordinal; replay restores `(session_id, event_ordinal)` order even when timestamps collide.
- `resume`/`continue` never automatically repeat an uncertain backend request or verification command. Stale source hashes, corrupt artifacts, cross-project bindings, and cross-session active workflow ownership fail closed before mutation.

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
