# State Lifecycle

This document defines cross-store state authority for `rolling-potato`.

It connects runtime ledger, SQLite projection, ontology graph, model knowledge
base, plugin registry, evidence artifacts, and current-state pointers. The goal
is to make replay, recovery, routing, and failure handling deterministic.

## Store Authority

| Store | Owns | Does not own |
| --- | --- | --- |
| append-only ledger | event order, audit trail, mutation intent | query performance |
| SQLite projection | query/index/reporting views | source-of-truth event order |
| current state | selected session/workflow pointers | historical truth |
| ontology graph | source-backed project semantics and invariants | model artifact trust |
| model manifest | model/backend artifact trust | runtime observations |
| model knowledge base | evidence index and repeated observations | install trust or default-model truth |
| plugin registry | imported plugin state and normalized capabilities | execution approval |
| evidence artifacts | redacted verification/debug pointers | policy decisions |

## Write Ordering

For state-changing operations:

1. Validate policy and project boundary.
2. Create a stable event id.
3. Append the ledger event.
4. Apply the state mutation.
5. Update SQLite/query projections.
6. Record evidence pointers.
7. Emit diagnostics or TUI updates.

If a later step fails, recovery replays from the ledger event and either
completes the projection or records a compensating failure event. Projection
writes must be idempotent by event id.

## Replay And Recovery

Replay rules:

- ledger events are replayed in event-time/order sequence
- runtime-ledger, project-session-ledger, and operation-log appends share one
  recoverable writer lease so concurrent processes cannot fork a hash chain
- projection rows use stable event ids for idempotency
- SQLite `ledger_events` and selectable session rows are rebuilt from the
  canonical runtime ledger; SQLite-only sessions are removed
- partial writes are detected by missing projection rows or mismatched hashes
- corrupt projection files are preserved before recreation
- current-state pointers are repaired only after ledger/session history is read
- ontology/model knowledge/plugin projections must not invent events during
  replay

## Routing Decision Record

Any runtime decision that selects a model, skill, mode, ontology view, backend,
subagent/team lane, or escalation target should write a routing decision record.

Required fields:

- user request id
- session id and workflow id
- selected skill and mode
- selected model, backend, quantization, and ontology view
- routing inputs: manifest status, model knowledge hints, benchmark evidence,
  user constraints, policy constraints, context budget
- rejected alternatives when relevant
- escalation target or fallback path
- final decision reason

The routing record is explainability evidence, not proof that the selected
model is globally best.

## Retry And Failure Handling

Failure handling depends on failure category.

| Failure class | Default action |
| --- | --- |
| model output failure | one bounded regeneration, then escalate or fail closed |
| prompt/context packing failure | rebuild context from source pointers, then retry once |
| ontology/source-pointer failure | reread source or block completion |
| runtime parser/policy failure | deny action and record validation gap |
| tool/command failure | classify idempotency before retry |
| backend/runtime failure | cleanup process, record diagnostic, retry only if safe |
| fixture/expected-output issue | quarantine fixture, do not score model |

Repeated invalid output narrows scope or escalates. The runtime must not keep
retrying until a risky action passes.

## Validation

Required tests:

- event id idempotent replay
- projection rebuild after corrupt SQLite
- partial-write recovery
- routing decision record creation
- model knowledge hint does not bypass manifest/policy
- retry budget stops repeated invalid output
- fixture issue is not scored as model failure

## v0.29.0 Workflow Checkpoints

Patch workflows live under `.rpotato/workflows/`. An immutable versioned snapshot,
its matching append-only `workflow.checkpoint` event, and the atomically replaced
committed-revision pointer jointly authorize resume. A synced transaction record
lets startup finish any interrupted snapshot/ledger/pointer window idempotently.
Every revision links `previous_hash` to `artifact_hash`; malformed ledger lines,
missing revisions, stale latest checkpoints, and chain conflicts fail closed.
Legacy schema v2 and v3 snapshots remain immutable and readable. A touched legacy
workflow appends its next revision as schema v4 while preserving the previous
artifact hash. Schema versions are monotonic and never downgrade.

Recovery scans every workflow pointer, transaction, and snapshot directory rather
than trusting only `current-state.json`. More than one nonterminal workflow is a
conflict and fails closed. A terminal workflow left in the active pointer after a
crash is revalidated and then cleared atomically. Patch approval and verification
approval are independent persisted gates. `patch approve` applies the bound patch
and stops at `pending-verification-approval`; only a separately issued credential
can authorize `patch verify`. Both pending gates, verification evidence, terminal
failure, and completion survive process restart. Resume never redisplays a
one-time credential or re-enters the model backend, and a completed resume reruns
proposal binding, source, evidence, and stop-gate checks.
`model-pending` and `action-recorded` recovery records a truthful terminal failure
instead of re-entering the backend. `verification-started` is an inconclusive
durable boundary: resume fails closed and requires a new explicit user-controlled
path rather than replaying the command. Approval/token rotation and workflow
checkpointing use PID/nonce recoverable leases plus workflow revision CAS.
Linux, macOS, and Windows liveness checks reclaim only a provably dead owner;
live or unknown owners fail closed. Explicit `cancel` restores hash-matched
applied bytes or records a durable conflict without replaying verification.

Corrupt workflow or ledger artifacts are preserved in place. A separate synced
validation-gap JSONL records only the failure class and an artifact descriptor
hash, so recovery never appends invented history to the damaged authoritative
ledger. Verification evidence uses deterministic IDs, atomic artifact replacement,
synced runtime append, and ledger-event deduplication. New ledger lines bind
physical append order with `previous_event_hash`/`event_hash`; a synced head
detects reorder, tamper, and tail truncation. An explicit legacy prefix is
accepted only before the chained suffix.

## v0.33.0 Skill State Checkpoints

Workflow schema v4 adds `active_skill_id`, invocation source, skill state,
completed lifecycle hooks, evidence keys, and satisfied stop criteria. The
canonical workflow snapshot and ledger checkpoint own this state; SQLite only
projects the active skill for queries and monitoring. Resume revalidates the
persisted skill contract before continuing or accepting a terminal workflow.
