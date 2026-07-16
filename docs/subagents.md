# Subagents

Subagents are bounded worker agents executed by the runtime core.

A subagent is not an independent process that ignores runtime state. It must inherit the parent workflow's policy, context limit, ledger requirements, and stop gate.

## Goals

- Split independent analysis or verification work.
- Keep each worker inside a narrow role and context.
- Improve small-model reliability through scope limits.
- Expand into multi-agent workflows without losing auditability.

## Non-Goals

- unbounded autonomous agents
- recursive orchestration by default
- parallel file writes without ownership
- hidden command execution
- separate policy engines per worker

## Roles

Initial roles:

- `explore`: repo lookup and source mapping
- `planner`: task decomposition
- `executor`: patch proposal
- `verifier`: tests, logs, evidence
- `critic`: risk and regression review
- `writer`: documentation and final report

A role is a capability constraint, not a personality label.

## Runtime Contract

Each subagent receives:

- parent workflow id
- role
- task slice
- allowed tools
- allowed paths
- context bundle
- output schema
- evidence requirements
- time/token budget

Each subagent returns:

- status: `complete`, `blocked`, `failed`, `cancelled`
- structured result
- evidence id
- suggested next action
- validation gaps

## Ownership

Subagents do not own global state.

Subagents may create:

- findings
- patches
- evidence
- summaries

Runtime core owns:

- action approval
- patch apply
- command execution
- merge decision
- stop gate

## Concurrency

Default execution is sequential. Parallel subagents are allowed only when work is independent.

Safe parallelism examples:

- one subagent maps repo structure while another reviews docs
- one subagent checks benchmark fixture design while another checks command policy
- one subagent verifies model manifest sources while another checks backend release artifacts

Examples that require serialization:

- two subagents editing the same file
- patch application and verification
- state migration and state reads

## Resource Admission

Subagent launch is subject to runtime admission control.

Admission inputs:

- available memory and backend health
- active model/backend process count
- parent workflow token/context budget
- per-subagent time/token budget
- file ownership conflicts
- command/tool permission risk
- current TUI or approval queue state

Default policy:

- do not load multiple local models only to run subagents in parallel
- prefer sequential execution when memory or context is constrained
- deny or defer subagents that would exceed token, time, memory, or ownership limits
- record admission decisions in the ledger
- failed admission narrows scope instead of silently dropping work

Current implemented slice:

- `rpotato team status` previews resource admission without mutation.
- `rpotato team admit --lanes <count>` records and enforces the resource lane gate.
- `rpotato team admit --lanes <count> --write <path> --command <command>` adds
  policy preflight for requested writes and commands. `ask` and `deny` decisions
  block dispatch until a later approval flow exists.
- `rpotato team admit --lanes <count> --write-owner <lane:path>` adds file
  ownership preflight. Normalized cross-lane write conflicts block dispatch
  before worker launch.
- Blocked policy/ownership admission writes `.rpotato/approval-requests/`
  records, and `rpotato tui approvals` displays those team requests next to
  patch proposal approvals.
- `rpotato team dispatch --lanes <count> --write-owner <lane:path>` rechecks
  normalized file ownership at the dispatch boundary, records dispatch status,
  and can record failed-worker continuation with `--failed-lane <lane>
  --failure <reason>`. It still does not launch workers.
- `rpotato team governor --lanes <count> --context-tokens <tokens>` records a
  context/model governor preflight. It clamps effective context tokens and emits
  local model-tier route hints without starting workers or claiming real model
  artifact capability.

## Failure Mode

Subagent failure must not damage parent state.

Rules:

- Failed subagent results are recorded in the ledger.
- Parent workflow may continue with reduced confidence.
- Without evidence, the stop gate cannot pass.
- Repeated failure narrows scope or asks the user.

## Validation

Subagent runtime needs tests for:

- role boundary enforcement
- path boundary enforcement
- shared file conflict detection
- parent cancellation propagation
- failed worker result handling
- merge evidence tracking
- resource admission denial and sequential fallback

## v0.35.0 Execution Contract

### Release Boundary

v0.35.0 adds one bounded child worker under one active parent workflow. It does
not add team-stage orchestration; that remains v0.36.0 work.

- One parent may have at most one non-terminal subagent.
- A subagent cannot launch another subagent or a team.
- The runtime prepares context, calls the backend, persists state, and merges
  evidence. The model never owns state or executes host side effects directly.
- Worker output may contain findings or a patch proposal. It cannot apply a
  patch, run a command, approve an action, or advance the parent workflow.
- v0.35.0 executes workers sequentially even when resource admission would
  permit parallel lanes.

### CLI Surface

```text
rpotato subagent launch --role <role> --task <text> \
  --tool <tool> --read <path> [--tool <tool>] [--read <path>] \
  [--write <path>] [--timeout-ms <ms>] [--max-tokens <tokens>]
rpotato subagent status [subagent-id]
rpotato subagent cancel <subagent-id>
```

`launch` requires the current session to have an active, non-terminal parent
workflow. The runtime binds the child to the exact parent revision and artifact
hash. `status` is read-only and defaults to the latest child of the active
parent. `cancel` is idempotent only for an already-cancelled child; other
terminal states are preserved and reported without mutation.

Initial role/tool policy:

| Role | Allowed tools | Result capability |
| --- | --- | --- |
| `explore` | `read_file` | findings |
| `planner` | `read_file` | plan and validation gaps |
| `verifier` | `read_file` | evidence-backed verdict |
| `critic` | `read_file` | ranked risks |
| `writer` | `read_file` | documentation result |
| `executor` | `read_file`, `render_diff` | one unapplied patch proposal |

Tools must be declared explicitly. `render_diff` requires at least one declared
write path, and every proposal target must be contained by one of those paths.
No command execution tool is available to a v0.35.0 worker.

### Bounded Inputs

- `task`: 1 to 4,096 UTF-8 bytes after trimming; raw task text is not written
  to the ledger or subagent state.
- `read` paths: 1 to 4 unique repository-relative paths. Paths are normalized,
  must stay inside the project root, and are re-read before backend dispatch.
- `write` paths: 0 to 4 unique normalized paths. They declare proposal
  ownership only and never authorize a write.
- Context: reuse the canonical context pack limits of at most 4 files and 3,200
  source characters; the child cannot expand the parent context budget.
- `timeout-ms`: default 30,000, valid range 1 through 300,000, matching the
  backend chat bound.
- `max-tokens`: default 256, valid range 1 through 1,024. The resource governor
  may lower the effective value but never raise it.
- Result artifact: at most 65,536 UTF-8 bytes before strict parsing.

Duplicate tools or paths, traversal, absolute paths outside the project,
unsupported roles, unknown tools, zero budgets, and excessive budgets fail
before a backend request.

### Persistent State

`SubagentRecordV1` is a canonical, hash-chained artifact with these fields:

- subagent id, revision, previous hash, and artifact hash
- project id, session id, parent workflow id, parent revision, and parent
  artifact hash
- role, task hash, declared tools, normalized read paths, and normalized write
  paths
- requested/effective token limit and timeout
- status, backend model-run event id, result artifact id/hash, evidence id/hash,
  failure code, and created/started/finished timestamps

The raw task, backend prompt, secrets, command output, and unredacted model
response are not ledger fields. State writes use a per-subagent recoverable
lease and compare-and-swap revision check.

Allowed transitions are closed:

| Current | Next |
| --- | --- |
| `requested` | `admitted`, `blocked`, `cancelled` |
| `admitted` | `running`, `cancelled` |
| `running` | `completed`, `blocked`, `failed`, `cancelled`, `timed-out` |
| terminal | none |

On restart, a stale `running` child becomes `failed` with
`interrupted-no-replay`. The runtime does not repeat the model request
automatically. A new attempt requires a new subagent id.

### Admission And Dispatch

Admission must pass all of these checks before `running` is recorded:

1. The parent identity, revision, and artifact hash still match the launch
   binding and the parent is non-terminal.
2. There is no other non-terminal child and the request is not recursive.
3. Role, tool, context, token, timeout, and result-size bounds are valid.
4. Read/write paths pass project-root normalization and declared ownership has
   no conflict.
5. The existing resource governor admits one sequential lane and the backend is
   healthy.
6. Context source bytes still match the source pointers captured for dispatch.

The runtime records `running` before calling the backend. Timeout uses the
existing bounded backend generation path. Manual cancellation of a running
child requests backend generation cancellation; a per-subagent state lease
ensures that completion and cancellation cannot both win.

### Strict Result And Parent Merge

`SubagentResultV1` is strict JSON with this logical schema:

- `schema_version: 1`
- `subagent_id`, `parent_workflow_id`, `role`, and terminal `status`
- bounded `summary`
- up to 16 bounded findings
- optional single patch proposal with target path, source hash, find text, and
  replacement text
- up to 16 evidence references and 16 validation gaps
- bounded suggested next action

Missing, duplicate, unknown, oversized, invalid UTF-8, or identity-mismatched
fields fail closed. An executor proposal outside declared write ownership or
against a changed source hash is blocked. Non-executor roles cannot return a
patch proposal.

Only a validated `completed` result may merge into the parent. Merge requires
the exact launch-bound parent artifact hash, appends the child evidence id to
the parent's skill evidence, checkpoints the parent once, and records one
idempotent merge event keyed by subagent id and result hash. A stale parent,
tampered child artifact, missing evidence, or a second different result never
mutates the parent.

Ledger event names are fixed:

- `team.subagent.requested`
- `team.subagent.admitted`
- `team.subagent.started`
- `team.subagent.completed`
- `team.subagent.blocked`
- `team.subagent.failed`
- `team.subagent.cancelled`
- `team.subagent.timed-out`
- `team.subagent.result-merged`

### Acceptance Tests

| ID | Required proof |
| --- | --- |
| S01 | CLI parses the complete launch/status/cancel surface and rejects missing or duplicate options. |
| S02 | Unknown roles, undeclared tools, and role/tool mismatches fail before backend dispatch. |
| S03 | Traversal, escaping absolute paths, duplicates, and more than four read/write paths fail closed. |
| S04 | Task, timeout, token, context, and result byte bounds are exact at minimum, maximum, and maximum-plus-one. |
| S05 | Missing, terminal, stale, cross-project, or cross-session parents cannot admit a child. |
| S06 | One non-terminal child blocks a second child and nested launch is always rejected. |
| S07 | Resource denial and ownership conflict record `blocked` without a backend request. |
| S08 | The context pack contains only declared, revalidated sources and stays within existing file/character limits. |
| S09 | The exact requested/admitted/started/completed event order and state revisions are deterministic. |
| S10 | Timeout records `timed-out`, requests generation cancellation, and never merges partial output. |
| S11 | Manual cancel and backend completion race to exactly one terminal state. |
| S12 | Restart converts stale `running` to `interrupted-no-replay` without a second model request. |
| S13 | Strict result parsing rejects unknown, missing, duplicate, oversized, invalid, and identity-mismatched fields. |
| S14 | Only executor may return one patch proposal, and its target/hash must match declared ownership and current bytes. |
| S15 | Completed evidence merges once; identical retry is a no-op and a different second result is blocked. |
| S16 | Stale parent revision/hash, tampered result/evidence, or missing evidence leaves the parent unchanged. |
| S17 | Status is bounded/read-only and reports requested/effective budgets, lifecycle, evidence, and failure code. |
| S18 | Ledger, state, transcript, diagnostics, and Korean CLI output do not expose raw task, prompt, secrets, or unredacted response. |

Implementation validation stays targeted until the full feature is stable:

```text
cargo test --locked subagent::tests::
cargo test --locked cli::tests::subagent
cargo test --locked --test subagent_lifecycle -- --test-threads=1
```

Delivery is split into four reviewable implementation units: contract/state,
CLI/admission/context, backend lifecycle/cancellation, and result/evidence
merge. The independent review and full repository gate run once after those
units are integrated on the final candidate commit.
