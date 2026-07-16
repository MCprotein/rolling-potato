# Code architecture

Korean: [코드 아키텍처](ko/code-architecture.md)

Migration ledger: [architecture-migration-map.json](architecture-migration-map.json)

This document is the canonical ownership and dependency contract for the
behavior-preserving v0.37.x refactor. [architecture.md](architecture.md)
continues to describe the product architecture; this document governs how the
Rust implementation is divided and how responsibilities migrate.

## Outcome and invariants

Before v0.38.0 development begins, every production and test responsibility is
owned by a cohesive domain boundary. Completion is defined by the migration
ledger reaching complete coverage with no compatibility facades, not by file
length or by a predetermined final patch number.

The refactor must preserve:

- CLI commands, arguments, output, and exit codes
- canonical durable bytes, record order, hashes, and event identity
- append, mutation, projection-lag, and recovery ordering
- ledger authority over rebuildable projections
- approval, security, permission, and fail-closed behavior
- backend, model, benchmark, and resource behavior
- release, packaging, checksum, and asset contracts

No new dependency, async runtime, actor system, public API, or user-visible
behavior is introduced by the refactor without a separately approved change.

## Ownership tree

```text
src/
  main.rs
  composition/                 wiring, startup/shutdown, command dispatch
  surfaces/
    cli/                       parse, command DTOs, presentation
    tui/                       controller, view model, rendering, input
  runtime_core/
    workflow/                  state, transitions, storage, recovery coordinator
    patch/                     intent, proposal, approval, apply, verify, rollback
    inference/                 backend, model, benchmark, resource policy
    extensions/                skill, hook, plugin lifecycle
    collaboration/             subagent and team lifecycle
    knowledge/                 context, evidence, ontology
    observability/             event projections, queries, monitoring
    policy/                    approval and fail-closed decisions
    reporting/                 surface-neutral report DTOs and invariants
  adapters/
    filesystem/                layout, atomic files, lease, cache, replacement
    process/                   child-process lifecycle
    sqlite/                    rebuildable observability projection
    llama_cpp/                 managed backend protocol and process adapter
    terminal/                  native terminal implementation
  foundation/                  typed, capability-independent primitives
```

Modules are private by default. A boundary becomes visible only to the narrowest
consumer that needs it. The v0.37.1 tree was documentation-only and reserved
ownership. Beginning with v0.37.2, production behavior moves only when its
ledger slice, targeted tests, and legacy-path removal close together.
The v0.37.3 inference boundary now owns backend/model/benchmark/resource rules,
durable inference codecs, and llama.cpp/process/filesystem implementations in
these private roots. The remaining top-level inference command/report modules
are compatibility facades scheduled for final composition cleanup in v0.37.13.
The v0.37.4 workflow storage-compatibility boundary now owns canonical workflow
snapshots and pointers, ledger event encoding/hashing/append, and transcript
record encoding/validation/install. Existing top-level modules retain path,
locking, transaction, recovery, projection, and command orchestration scheduled
for their later migration slices.
The v0.37.5 workflow domain boundary now owns current-state and lease DTOs,
session-resume authority checks, read-only snapshot/checkpoint binding, canonical
transcript-session ordering and duplicate rejection, transcript event/record
binding, and tool-output view DTOs. The top-level state and transcript modules
remain compatibility facades for filesystem, lock, transaction, recovery,
projection, and command orchestration assigned to later slices.
The v0.37.6 workflow application boundary now owns legal transition records,
exact event progression, state/checkpoint/reconcile/approval/verification/
terminal transaction order, prepared workflow recovery, current-state recovery,
and the projection-lag recovery barrier. Top-level state, ledger, and transition
modules provide concrete filesystem, lock, event-sink, and cleanup operations;
they no longer select the migrated commit or recovery sequence.
The v0.37.7 observability boundary now owns surface-neutral projection records
and ports plus monitor query/report use cases. SQLite adapters own rebuildable
schema, replay, query, ledger-validation, and transcript-row operations, while
the canonical ledger and transcript remain the only durable authorities. The
top-level observability and monitor facades remain staged compatibility paths
until the v0.37.13 composition cleanup.
The v0.37.11 extension boundary owns deterministic hook policy, skill manifest
and lifecycle policy, and plugin manifest/capability admission rules. The
top-level hook, skill, and plugin facades retain only concrete ledger/state,
workflow persistence, filesystem snapshot, and discovery integration until the
v0.37.13 composition cleanup.
The v0.37.12 collaboration boundary owns bounded subagent launch and result
policy; team admission, continuation, stage, and execution decisions; canonical
team manifest/state codecs; action ownership; and deterministic reconciliation
rules and artifacts. Top-level subagent and team facades retain concrete
backend, thread, lease, filesystem, recovery, ledger/projection, and workflow
checkpoint integration until the v0.37.13 composition cleanup. The root
`subagent_lifecycle` and `team_runtime` Cargo harnesses delegate to grouped
integration bodies under `tests/collaboration`.

## Dependency direction

Allowed directions are:

```text
main -> composition
composition -> surfaces + runtime_core facades + concrete adapters
surfaces -> runtime_core use-case/query DTOs + foundation
runtime_core application -> owning domain + consumer-owned ports
runtime_core domain -> foundation
adapters -> consumer-owned ports + foundation
cross-context -> owning facade or canonical DTO/event
foundation -> std + already-present low-level crates
```

The following edges are forbidden:

- surfaces importing concrete adapters
- domain code importing filesystem, process, SQLite, terminal, CLI, or TUI code
- adapters calling surface or report renderers
- one context directly reading another context's files or tables
- generic `utils`, `services`, or `managers` ownership buckets
- blanket traits without substitution or invariant evidence
- async, Tokio, or actors without a measured concurrent-I/O requirement

The architecture contract test scans imports inside the new roots. A temporary
exception must record an owner, rationale, and v0.37.x expiry release in the
migration ledger.

## Rust design rules

- Use newtypes for identifiers, hashes, versions, and validated paths that can
  otherwise be mixed accidentally.
- Use enums and exhaustive matches for closed command, event, and state sets.
- Use typestate only for stable process-local construction. Persisted and
  recovered incomplete states remain validated enums or records.
- Define a trait beside its consumer and only for a real substitution seam or
  invariant boundary.
- Prefer owned domain vocabulary over generic service/repository abstractions.
- Keep the runtime synchronous throughout this refactor.
- Keep private rule tests next to their owner and boundary contracts in
  integration tests.

## Durable workflow boundary

The byte-compatible `WorkflowRecord`, ledger events, and transcript records now
have one canonical codec owner under `runtime_core/workflow/storage_compat`.
Domain views and commands validate those records; they do not redefine or
independently serialize them during the train. Storage compatibility integration
tests lock workflow snapshot/pointer bytes, ledger append order/hash chains and
failure boundaries, and transcript exact/idempotent/immutable installation.

One workflow application transaction coordinator owns cross-store ordering:

1. canonical append
2. state mutation
3. project/global log convergence
4. projection-lag barrier
5. recovery and cleanup

Ports and adapters participate in those operations but never choose their order.

## Migration method

Every governed file has exactly one file record in the
[migration ledger](architecture-migration-map.json). A file may have multiple
non-overlapping responsibility slices. Each slice has one responsibility, one
exact target, one v0.37.x release, one state, and an evidence list.

The separate responsibility inventory is the completeness oracle: every
inventory responsibility must be claimed by exactly one slice, and every slice
must correspond to one inventory responsibility. Complete evidence must name an
existing proof path or a declared logical proof ID.

Allowed states are `planned`, `migrating`, `compatibility-facade`, `complete`,
and `exception`. Exceptions additionally require an owner, rationale, and expiry
release. The ledger's `current_release` makes expired exceptions fail the
contract. Setting `train_completion` to `true` rejects every state except
`complete`, which makes planned work, active migration, exceptions, and
compatibility facades release blockers. A migration is complete only after its
targeted tests pass, its legacy facade is removed, and its PR/release evidence
is recorded.

Each logical unit closes as:

```text
lock missing behavior -> move one responsibility -> targeted verify -> commit -> push
```

The stabilized patch release receives one bounded independent review. Full
format, test, clippy, release build, and release-policy verification runs once
for the exact candidate commit in PR CI. Platform packaging and release-asset
smoke remain tag-time deployment verification.
