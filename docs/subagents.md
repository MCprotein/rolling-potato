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
