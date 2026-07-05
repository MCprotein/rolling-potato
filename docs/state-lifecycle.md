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
- projection rows use stable event ids for idempotency
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
