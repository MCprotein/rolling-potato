# Team Runtime

Team runtime is a runtime capability that coordinates multiple subagents under one parent workflow.

It is the path for work where parallel or staged execution materially helps. It is not the default path for small patch tasks.

## Goals

- Support Claude Code/Codex replacement-level workflows.
- Coordinate multiple bounded agents.
- Keep one runtime policy engine.
- Make team work resumable and auditable.
- Prevent worker conflicts and hidden side effects.

## Team Pipeline

Default staged pipeline:

1. `team-plan`
2. `team-dispatch`
3. `team-exec`
4. `team-review`
5. `team-verify`
6. `team-merge`
7. `team-report`

Each stage is a runtime state transition.

## Team Manifest

Team execution should have a manifest.

```json
{
  "schemaVersion": 1,
  "teamId": "fix-regression-team",
  "parentWorkflowId": "workflow-123",
  "members": [
    {"id": "explore-1", "role": "explore"},
    {"id": "executor-1", "role": "executor"},
    {"id": "verifier-1", "role": "verifier"}
  ],
  "writePolicy": "single_writer",
  "mergePolicy": "runtime_owned",
  "stopGate": "evidence_required"
}
```

## Write Policy

Default write policy:

- Subagents can propose patches.
- Runtime core applies patches.
- Only one writer can own a file at a time.
- Conflicts escalate to the parent workflow.
- Verification runs after ownership is resolved and merge is complete.

## Coordination Rules

- Parent workflow owns the global plan.
- Workers execute only assigned slices.
- Workers cannot spawn teams by default.
- Workers cannot widen their own scope.
- Team state is recorded in the ledger.
- Team cancellation propagates to all active workers.

## Resource Admission

Team mode is admitted only when runtime resources can support it.

Admission checks:

- one model/backend sidecar is reused unless a later backend policy explicitly allows more
- worker count fits available memory, token budget, context budget, and timeout
- file ownership can be assigned before dispatch
- approval queue and TUI state can represent all pending decisions
- plugin/tool permissions required by workers are known before dispatch

If admission fails, the runtime should fall back to sequential subagents or a
single-agent workflow and record the reason in the ledger. Team admission must
not silently drop assigned work.

`rpotato team status` is the current read-only admission preview. It consumes
the latest resource sample and reports whether a future team dispatch would be
parallel, sequential fallback, or blocked. It does not start workers or mutate
workflow state yet.

`rpotato team admit --lanes <count>` is the first enforced admission gate. It
uses the same resource policy but records a ledger event and returns a blocked
error on critical pressure. Normal pressure admits the requested lane count;
unknown or degraded pressure falls back to one sequential lane. This command
still does not start workers or advance team stages, so future dispatcher work
can attach worker launch after the gate without changing the admission
contract.

`team admit` can also preflight requested write paths, lane ownership, and
commands:

```text
rpotato team admit --lanes 2 --write README.md --command "cargo test"
rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs
```

The preflight uses the shared runtime policy engine. `allow` checks can pass the
gate; `ask` and `deny` checks block dispatch and are recorded in the same
admission ledger event. `--write-owner <lane:path>` additionally normalizes
lane-owned write paths before dispatch. If two lanes claim the same normalized
path, admission returns an ownership-blocked result and records it in the same
ledger event. This is still admission-time preflight, not worker launch or
merge-time ownership enforcement.

## TUI Integration

TUI should show:

- team stage
- worker status
- active task slice
- pending approvals
- conflicts
- evidence status
- final merge status

TUI displays team state. It is not the coordination authority.

## Validation

Team runtime needs tests for:

- team manifest parsing
- worker lifecycle state transition
- cancellation propagation
- shared file conflict
- failed worker continuation
- merge gate
- evidence-required stop gate
- team resource admission and sequential fallback
