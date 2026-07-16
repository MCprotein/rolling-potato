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

Team execution starts from a canonical compact JSON manifest whose keys use the
exact order shown below. `parent_workflow_id` must name the active non-terminal
workflow. Lanes are consecutive from 1, and each member carries the same
bounded role/tool/path/token contract used by the subagent runtime.

```json
{"schema_version":1,"team_id":"fix-regression-team","parent_workflow_id":"workflow-123","members":[{"lane":1,"id":"explore-1","role":"explore","task":"map the affected files","tools":["read_file"],"read_paths":["src"],"write_paths":[],"timeout_ms":30000,"max_tokens":256}],"write_policy":"single_writer","merge_policy":"runtime_owned","stop_gate":"evidence_required"}
```

```text
rpotato team plan --manifest plans/team.json
```

`team plan` validates the manifest, rejects cross-lane write ownership, binds
the plan to the exact parent revision/hash, installs the manifest under
`.rpotato/teams/`, and creates revision 1 of the hash-chained `team-plan`
state. Retrying the same plan is idempotent. It does not start a worker or
advance to `team-dispatch`; `team execute` consumes this durable state.
`team status` exposes the latest team id, stage, status, revision, and execution
mode for the active parent.

## Worker Execution

```text
rpotato team execute --team fix-regression-team
```

`team execute` verifies the exact state, manifest, parent workflow, project,
session, and backend bindings before admitting workers. Under normal resource
pressure it checkpoints every admitted member as running and executes their
bounded backend generations in parallel. Under unknown or degraded pressure it
uses one admitted lane but still executes every manifest member sequentially;
assigned work is never silently dropped. Critical pressure blocks before worker
admission or a team stage transition.

Write ownership rejects both identical paths and ancestor/descendant overlaps
across lanes. When an executor returns a patch action, the runtime reloads its
immutable result artifact and rechecks the member identity, launch contract,
target path, and source hash against the canonical manifest owner. Successful
checks emit `team.worker.action-owned`; an ownership or worker failure is
recorded, all already-admitted lanes are collected, and the durable team state
transitions to `failed` without merging partial results into the parent.

Successful worker results and evidence are stored as immutable subagent
artifacts and the durable team state advances through `team-dispatch` to
`team-exec`. Workers do not merge evidence into the parent independently. The
parent revision and evidence remain unchanged until a later reconciliation
stage validates and merges the complete team result set. `team dispatch`
remains the older standalone preflight/reporting command described below; it is
not an alias for `team execute`.

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
the latest resource sample, reports whether a future team dispatch would be
parallel, sequential fallback, or blocked, and surfaces the latest `team.*`
runtime ledger event for the current project. It does not start workers or
mutate workflow state yet.

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

`rpotato team dispatch --lanes <count> --write-owner <lane:path>` is the first
dispatch-time hardening surface:

```text
rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
```

It reuses the resource lane decision and normalized file ownership rules at the
dispatch boundary. Cross-lane ownership conflicts, invalid failed lanes, and
critical resource pressure return blocked errors and record ledger/SQLite
projection events. `--failed-lane <lane> --failure <reason>` records whether
the runtime can continue with the remaining admitted lanes. This command is
still a preflight/reporting surface: it does not launch subagents, execute
tools, merge files, or advance team stages.

When policy or ownership preflight blocks admission, the runtime also writes a
redacted project-local approval request under `.rpotato/approval-requests/`.
`rpotato tui approvals` renders these team admission requests beside patch
proposal approvals. The TUI remains read-only; approval execution and worker
dispatch are still separate future gates.

`rpotato team governor --lanes <count> --context-tokens <tokens>` is the first
context/model governor preflight:

```text
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
```

It consumes the latest resource sample, reports admitted lanes, clamps effective
context tokens against the configured budget and pressure state, records a
ledger event, and emits local model-tier route hints: `keep`, `downgrade`,
`escalate`, or `defer`. These are runtime policy hints only. They do not claim
real model capability, download/select model artifacts, or start workers.

## TUI Integration

TUI should show:

- team stage
- worker status
- active task slice
- pending approvals
- team admission approval requests
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
