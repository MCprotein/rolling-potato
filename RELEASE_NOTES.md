# Release Notes

## v0.22.0 - Dispatcher Hardening

Release date: 2026-07-09

This release adds the first dispatch-time team hardening surface. It records
dispatch ownership decisions and failed-worker continuation state without
starting workers or advancing team stages.

### Included

- New `rpotato team dispatch --lanes <count> --write-owner <lane:path>`
  command.
- Dispatch-time normalized file ownership enforcement. Cross-lane ownership
  conflicts and critical resource pressure return blocked errors and record
  ledger/SQLite projection events.
- Failed-worker continuation recording with
  `--failed-lane <lane> --failure <reason>`, including remaining admitted lane
  count and continuation action.
- `rpotato team status` now surfaces the latest `team.*` runtime ledger event
  for the current project.
- English and Korean docs now describe `team dispatch` as a preflight/reporting
  boundary, not a worker launcher.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (196 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs`
- `rpotato team status`

### Boundary

This release does not launch subagents, execute tools, merge worker output,
advance team stages, or implement a full dispatcher. It only records and
enforces the dispatch preflight state needed before worker launch can exist.

## v0.21.0 - Benchmark-Driven Optimization Policy

Release date: 2026-07-09

This release adds the first read-only optimization policy surface. It consumes
local runtime metrics and local benchmark evidence to recommend safer context,
team-lane, fallback, and model-route hints for small-model execution.

### Included

- New `rpotato monitor optimize` command.
- Deterministic optimization policy over resource pressure, model-run metrics,
  context clamp count, p95 latency, average tokens/sec, and local benchmark
  pass/fail evidence.
- `monitor optimize` reports measured benchmark run count, pass/fail count,
  average local score, latest measured benchmark row, recommended context
  tokens, recommended team lanes, fallback mode, and model route hint.
- Recommendations are read-only local runtime hints. They do not choose a real
  model artifact, promote model status, store raw prompt/source text, or claim
  public benchmark parity.
- English and Korean docs now describe `monitor optimize` as the v0.21.0
  benchmark-driven optimization policy surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (189 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`
- `rpotato monitor optimize`

### Boundary

This release does not implement dispatcher worker launch, model promotion,
public benchmark parity, or automatic model selection. It only converts local
SQLite projection evidence into conservative runtime policy hints.

## v0.20.1 - Benchmark Evidence Status

Release date: 2026-07-09

This patch release records the first real Qwen executable smoke measurement and
fixes the model evaluation preflight so it reflects locally measured benchmark
rows.

### Fixed

- `rpotato model eval-plan qwen3.5-4b` now reports the latest local
  `measured-locally` benchmark row from the SQLite `benchmark_runs` projection
  instead of always showing `local benchmark status: not-run`.
- The status advances to `local-smoke-measured` when a measured row exists for
  the candidate artifact model id.

### Evidence Recorded

- Qwen3.5-4B Q4_K_M local artifact was already present and SHA-256 verified.
- Managed `llama.cpp` version `9878 (2da668617)` started the Qwen sidecar with
  `--ctx-size 4096`.
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json
  --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` recorded
  score `3/3`, `local_pass=true`, latency `243ms`, total tokens `83`, resource
  pressure `normal`, and peak RSS `3351363584` bytes.
- The sidecar was stopped after measurement.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (186 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `rpotato model eval-plan qwen3.5-4b`
- `rpotato backend status`

### Boundary

This is a local smoke benchmark only. It does not promote Qwen3.5-4B to
`verified` and does not claim public benchmark parity.

## v0.20.0 - Executable Benchmark Runner

Release date: 2026-07-09

This release adds the first executable local benchmark runner. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
  [--max-tokens <tokens>]` command.
- `benchmark run` calls the currently running backend sidecar and records a
  local `claim_state=measured-locally` benchmark row.
- Deterministic 0-3 local product score based on expected/forbidden response
  markers, abstention requirement, and non-empty model output.
- SQLite migration v4 extends `benchmark_runs` with `model_run_id`, prompt
  artifact checksum/length, local pass flag, marker counts, latency, token
  counts, resource pressure, and peak RSS.
- `benchmark report --format jsonl` exports the new executable benchmark fields.
- `benchmarks/fixtures/executable-smoke.json` and
  `benchmarks/prompts/executable-smoke.txt` provide the first executable smoke
  fixture/prompt pair.
- English and Korean documentation updates for executable benchmark boundaries,
  redaction, and observability linkage.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (185 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark validate benchmarks/fixtures/executable-smoke.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` fail-closed without a running sidecar
- `rpotato benchmark report --format jsonl`

### Known Issues

- `benchmark run` requires an already running backend sidecar and a local model
  file started through `rpotato backend start`; this release does not bundle or
  auto-select model weights.
- The executable runner records local product scores only. It does not compare
  against public benchmark scores or claim leaderboard parity.
- Source-read compliance and hallucination scoring are still marker/proxy based;
  richer tool/evidence-aware scoring remains planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.19.0 - Benchmark Harness Foundation

Release date: 2026-07-09

This release adds the first metadata-only benchmark harness surface. It is still
a source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato benchmark validate <fixture.json>` command.
- New `rpotato benchmark record --fixture <fixture.json>` command.
- New `rpotato benchmark report --format jsonl` command.
- Project-local fixture schema validation for runtime capability, model/runtime
  responsibility, expected route, policy decision, escalation target, required
  tool/source/evidence records, abstention requirement, ontology view, context
  budget, backend/model artifact identifiers, sampling policy, and raw artifact
  retention policy.
- SQLite migration v3 extends `benchmark_runs` with session, fixture checksum,
  claim state, reproducibility manifest, and redacted report fields.
- Metadata-only benchmark records use `claim_state=not-comparable` and
  `score=null`; no model execution or public benchmark parity claim is made.
- `benchmarks/fixtures/sample.json` provides a no-raw-prompt/source CLI-contract
  smoke fixture.
- English and Korean documentation updates for benchmark fixture contracts,
  observability integration, and v0.19.0 rollout status.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (183 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark report --format jsonl`
- `rpotato monitor status`

### Known Issues

- Benchmark commands do not execute models, score fixtures, or compare local
  scores with public benchmarks.
- Hardware/RAM/power/thermal manifest fields are present only as
  `not-recorded` placeholders until executable benchmark runs collect them.
- Fixture suites, ontology-view scoring, public benchmark parity reports, and
  benchmark-driven optimization policy remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.18.0 - Performance Baseline Report

Release date: 2026-07-08

This release adds a read-only local performance baseline report. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato monitor baseline` command.
- Aggregates existing local ledger/SQLite projection metrics without adding a
  new raw prompt/source store.
- Reports p50/p95 latency, average tokens/sec, context clamp count, context
  tokens dropped, peak RSS, pressure-state distribution, and
  model/backend/session grouping.
- Keeps the report as local metric evidence only; it does not select model
  artifacts or make source-backed model capability claims.
- English and Korean documentation updates for the v0.18.0 performance
  baseline scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (172 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`

### Known Issues

- `monitor baseline` reports only metrics already present in the local
  projection. It does not run benchmarks or collect continuous background
  samples.
- Benchmark harness recording, redacted report export, and benchmark-driven
  optimization policy remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.17.0 - Team Context And Model Governor

Release date: 2026-07-08

This release adds the first team context/model governor preflight. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato team governor --lanes <count> --context-tokens <tokens>`
  command.
- Optional `--context-limit <tokens>` and `--model-tier small|standard|large`
  inputs for explicit runtime policy simulation.
- Latest resource sample consumption for admitted-lane and context/model
  governor decisions.
- Effective context-token clamp against the configured budget, degraded-pressure
  budget, and local small-model soft budget.
- Local model route hints: `keep`, `downgrade`, `escalate`, and `defer`.
- Ledger/SQLite recording for team governor decisions.
- English and Korean documentation updates for the v0.17.0 governor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (170 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard`
- `rpotato team governor --lanes 2 --context-tokens 1024 --context-limit 4096 --model-tier small`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that normal pressure records a clamped context/model decision while critical
pressure blocks with a `defer` route hint.

### Known Issues

- `team governor` is a preflight/reporting surface. It does not start workers,
  select real model artifacts, or execute model routing.
- Model route hints are local runtime policy hints only; they are not
  source-backed claims about any real model artifact's capability.
- Dispatch-time ownership enforcement and failed-worker continuation remain
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.16.0 - Team Approval Queue Integration

Release date: 2026-07-08

This release connects blocked team admission decisions to the read-only approval
queue. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- New project-local approval request store under `.rpotato/approval-requests/`.
- Blocking `team admit` policy/ownership decisions now write redacted approval
  request records linked to the team admission ledger event.
- `rpotato tui approvals` now renders team admission approval requests beside
  patch proposal approvals.
- `rpotato init` creates the approval request directory as part of the project
  runtime layout.
- Team admission output includes the approval request id and path when a policy
  or ownership decision needs review.
- English and Korean documentation updates for the v0.16.0 approval queue
  integration scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (165 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato tui approvals`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that policy/ownership-blocked team admission records appear in the read-only TUI
approval queue.

### Known Issues

- `tui approvals` is read-only. It lists team admission requests but does not
  approve, deny, or resume dispatch.
- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or enforce ownership during actual worker execution.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.15.0 - Team File Ownership Preflight

Release date: 2026-07-08

This release adds file ownership preflight to the enforced team admission gate.
It is still a source-only developer preview: it does not ship model weights,
external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato team admit --lanes <count>` now accepts repeated
  `--write-owner <lane:path>` ownership claims.
- Ownership paths are normalized before dispatch so equivalent paths such as
  `README.md` and `./README.md` resolve to the same ownership key.
- Cross-lane ownership conflicts block admission before any future worker
  launch.
- Owned write paths also participate in the existing write policy preflight, so
  approval-required writes still block dispatch until approval queue integration
  exists.
- Team admission output and ledger event details include ownership claim count,
  ownership status, ownership blocked flag, owned write paths, and per-claim
  decisions.
- English and Korean documentation updates for the v0.15.0 ownership preflight
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (163 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that distinct lane-owned paths are allocated while normalized cross-lane
ownership conflicts block dispatch before worker launch.

### Known Issues

- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or enforce ownership during actual worker execution.
- `ask` decisions block dispatch because approval queue integration is still
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.14.0 - Team Policy Preflight

Release date: 2026-07-08

This release adds policy preflight to the enforced team admission gate. It is
still a source-only developer preview: it does not ship model weights, external
plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato team admit --lanes <count>` now accepts repeated `--write <path>` and
  `--command <command>` preflight checks.
- Requested write paths are classified with the same policy engine as
  `policy check-path --write`.
- Requested commands are classified with the same policy engine as
  `policy check-command`.
- `allow` policy checks can pass the admission gate.
- `ask` and `deny` policy checks block dispatch before any future worker launch.
- Team admission output and ledger event details include policy check count,
  policy status, policy blocked flag, requested writes, redacted commands, and
  per-check decisions.
- English and Korean documentation updates for the v0.14.0 policy preflight
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (159 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that command preflight can pass while write preflight blocks with
`approval-required` before worker launch.

### Known Issues

- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or allocate file ownership.
- `ask` decisions block dispatch because approval queue integration is still
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.13.0 - Team Admission Gate

Release date: 2026-07-07

This release turns the v0.12.0 read-only team admission preview into the first
enforced admission gate. It is still a source-only developer preview: it does
not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- New `rpotato team admit --lanes <count>` command.
- Admission decisions are recorded in the append-only ledger and SQLite
  projection.
- Normal pressure admits the requested parallel lanes.
- Missing/unknown or degraded pressure falls back to one sequential lane.
- Critical pressure returns a blocked error before any future worker launch.
- `team status` remains read-only; `team admit` is the mutating gate.
- English and Korean documentation updates for the v0.13.0 admission gate
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (157 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that `team admit` records a ledger event while falling back to one sequential
lane when no resource sample exists.

### Known Issues

- Policy preflight for requested writes and commands is introduced in v0.14.0.
  Full worker dispatch and file ownership allocation remain planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp, file ownership, tool risk, approval queue, and model
  downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.12.0 - Team Admission Preview

Release date: 2026-07-07

This release adds the first read-only team admission surface on top of the
resource monitoring/governor work. It is still a source-only developer preview:
it does not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- New `rpotato team status` command.
- Reusable resource lane admission policy for future subagent/team dispatch.
- Normal pressure admits the requested parallel lanes.
- Missing/unknown or degraded pressure falls back to one sequential lane.
- Critical pressure blocks new team dispatch.
- `team status` reports latest resource sample metadata, requested lanes,
  admitted lanes, admission, dispatch-blocked flag, fallback, reason, hint, and
  read-only boundary.
- English and Korean documentation updates for the v0.12.0 team admission
  preview scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (153 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that `team status` reports sequential fallback without mutating workflow state
when no resource sample exists.

### Known Issues

- `team status` is an admission preview only; it does not start subagents,
  dispatch team lanes, mutate workflows, or enforce file ownership yet.
- Resource sampling is still event-driven, not continuous live polling.
- Enforced resource admission gate is introduced in v0.13.0; remaining
  dispatcher policy stays planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.11.0 - Backend Chat Resource Governor

Release date: 2026-07-07

This release adds the first runtime resource governor slice for the managed
backend sidecar. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato backend chat` now samples backend CPU/RSS/disk resource pressure
  before model execution.
- Critical resource pressure blocks chat before the `/v1/chat/completions`
  request is sent.
- Degraded resource pressure clamps the effective max-token budget while
  preserving normal and unknown-pressure requests.
- `backend chat` and `run` output now distinguish requested max tokens from
  effective max tokens and report the governor admission/token action.
- Redacted ledger events record governor admission, token action, reason, and
  sample event ids without storing raw prompts or raw responses.
- English and Korean documentation updates for the v0.11.0 governor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (149 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato backend chat --prompt smoke --max-tokens 256`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp`; without a
running backend sidecar, `backend chat` must fail closed before model execution
and must not create raw prompt/response storage.

### Known Issues

- Resource sampling is still event-driven, not continuous live polling.
- The v0.11.0 governor applies to backend chat only. Team admission preview and
  sequential fallback are introduced in v0.12.0; enforced subagent/team
  dispatch admission remains planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.10.0 - TUI Resource Monitor

Release date: 2026-07-07

This release extends the read-only TUI beta with a resource-pressure monitor for
the managed backend sidecar. It is still a source-only developer preview: it
does not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- `rpotato tui monitor` now shows resource sample count, latest pressure status,
  CPU percent, average/peak RSS, disk bytes, and recorded timestamp.
- Model monitoring summaries now include average tokens per second alongside
  total tokens and average latency.
- The monitor layout stays dependency-free and terminal-safe, including narrow
  `COLUMNS=64` rendering.
- English and Korean documentation updates for the v0.10.0 TUI monitor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (148 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui monitor`
- `COLUMNS=64 rpotato tui monitor`

The TUI smoke used a scratch project root under `/private/tmp`, initialized
runtime state with observability schema v2, and verified that the monitor view
renders resource pressure, resource sample count, model/token counts, read-only
actions, and the beta boundary without mutating workflow state.

### Known Issues

- Resource monitor data is event-driven and reflects the latest recorded sample;
  it is not continuous live polling.
- Runtime resource governor behavior is not included in v0.10.0; the first
  backend chat governor slice is introduced in v0.11.0.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.9.0 - Backend Resource Sampling

Release date: 2026-07-07

This release adds the first backend resource monitoring slice for the managed
`llama.cpp` sidecar. It is still a source-only developer preview: it does not
ship model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `resource_samples` SQLite projection schema with CPU percent, average/peak
  RSS bytes, disk bytes, sample count, pressure status, and recorded timestamp.
- Backend resource sampling on `backend start`, already-running start reuse,
  `backend status` for running sidecars, and `backend chat`.
- Redacted `backend.resource.sampled` ledger events; raw prompts, responses, and
  source text are still not persisted by default.
- `monitor status` now shows resource sample counts and the latest sampled CPU,
  RSS, disk, and pressure fields.
- `monitor prune --dry-run` now includes resource sample row counts.
- English and Korean documentation updates for the v0.9.0 monitoring scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (147 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato monitor status`
- `rpotato backend status`
- `rpotato monitor prune --before 30d --dry-run`

The CLI smoke used a scratch project root under `/private/tmp`, initialized
runtime state with observability schema v2, and verified that monitor output
includes resource sample count plus latest resource CPU/RSS/disk/pressure
fields.

### Known Issues

- Resource sampling is event-driven, not continuous background polling.
- TUI resource-pressure display is not included in v0.9.0; it is introduced in v0.10.0.
- Runtime resource governor behavior is not included in v0.9.0; the first
  backend chat governor slice is introduced in v0.11.0.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.8.0 - TUI Evidence And Stop Gate View

Release date: 2026-07-07

This release extends the read-only TUI beta with evidence and stop-gate status
inspection. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui evidence` shows runtime evidence store paths, runtime evidence
  record counts, project evidence artifact counts, SQLite evidence record
  counts, SQLite stop-gate result counts, and the stale evidence policy summary.
- The TUI overview now points to the evidence view.
- `monitor status` now includes SQLite evidence and stop-gate result counts.
- Read-only evidence store status API with project-local artifact counting.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (143 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui evidence`
- `COLUMNS=64 rpotato tui evidence`

The TUI smoke used a scratch project root under `/private/tmp`, initialized
runtime state, and rendered the evidence view with runtime evidence,
project-evidence, observability, stop-gate count, stale-policy, validation
command, and read-only beta-boundary fields.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- The evidence view reports evidence/stop-gate status only; it does not pass or
  fail workflows.
- Terminal stop-gate evaluation, tool output viewer, subagent/team status, and
  plugin permission review remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.7.0 - TUI Session Transcript View

Release date: 2026-07-07

This release extends the read-only TUI beta with selected-session event
inspection. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui transcript <session-id>` shows selected-session metadata and a
  timestamp-ordered event timeline.
- `rpotato tui sessions` now points users to the transcript inspection command.
- SQLite observability read API for session events.
- Read-only boundary that keeps transcript replay, resume, cancellation, and
  workflow mutation out of the TUI beta.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (140 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato session new`
- `rpotato state resume`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `COLUMNS=64 rpotato tui transcript <session-id>`

The TUI smoke used a scratch project root under `/private/tmp`, created a new
session, recorded a no-op resume event, listed the session, and showed the two
projected ledger events in the transcript timeline without replaying raw model
transcripts or mutating workflow state.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- The transcript view shows projected ledger event metadata and summaries only;
  raw event details and model transcript replay remain future agent-loop work.
- Tool output viewer, subagent/team status, plugin permission review, and
  stop-gate evidence views remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.6.0 - TUI Approval And Diff Views

Release date: 2026-07-07

This release extends the read-only TUI beta with patch approval queue and diff
inspection views. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui approvals` lists project-local patch proposal records.
- `rpotato tui diff <proposal-id>` shows proposal metadata, approve/dry-run
  command hints, and the stored unified diff.
- Patch proposal read APIs for summaries and details.
- Literal diff rendering in the TUI so `---`, `+++`, `@@`, `-`, and `+` lines
  remain readable in terminal output.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (138 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path src/lib.rs --find 1 --replace 2`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `COLUMNS=64 rpotato tui diff <proposal-id>`

The TUI smoke used a scratch project root under `/private/tmp`, created a patch
proposal, rendered it as a pending approval record, and showed the stored
unified diff without applying or approving the patch.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- Approval queue and diff views inspect existing patch proposal records only;
  approval and apply still happen through `rpotato patch approve`.
- Transcript view, tool output viewer, subagent/team status, plugin permission
  review, and stop-gate evidence views remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.5.0 - Read-Only TUI Beta

Release date: 2026-07-07

This release adds the first read-only TUI beta surface for terminal-only
environments. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui` overview dashboard
- `rpotato tui monitor` model/token monitoring view
- `rpotato tui sessions` session-history view with full session ids and resume
  hint
- Dependency-free ASCII layout for SSH/Linux-server friendly rendering
- Read-only boundary that does not approve, apply, resume, cancel, or mutate
  workflows
- English and Korean documentation updates for the TUI beta surface

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (133 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

The TUI smoke showed project/session state, SQLite observability path, recorded
model/token metrics, session history, and the read-only beta boundary.

### Known Issues

- The TUI beta is a one-shot read-only render, not an interactive event loop.
- Approval queue, diff viewer, transcript view, subagent/team status, plugin
  permission review, and stop-gate evidence views remain future work.
- The first beta intentionally avoids a TUI framework dependency; a richer TUI
  crate can be reconsidered after interaction requirements stabilize.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.4.0 - Approved Patch Apply

Release date: 2026-07-07

This release extends the patch approval surface from dry-run gate checks to
approved patch application with rollback records and optional verification
command execution. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato patch approve <proposal-id> --token <token>` applies an approved
  proposal without `--dry-run`
- Current-file SHA-256 guard before apply, blocking stale proposals when the
  target file changed after preview
- Rollback record creation under `.rpotato/patch-proposals/`
- Applied SHA-256 verification after write
- `--verify-command <command>` for allow-listed simple argv verification
  commands after apply
- Verification failure handling that attempts rollback and refuses success
  reporting
- English and Korean documentation updates for the new patch application
  boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (127 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- Scratch-project smoke with `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke`
- `rpotato patch preview --path README.md --find "Local coding agents for potato PCs." --replace "Local coding agents for potato PCs. Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --verify-command "rg Smoke README.md"`

The patch smoke returned `status: applied`, wrote a rollback record, returned
`verification status: passed`, and reported verification exit code `0`. The
smoke ran against a `/private/tmp` project fixture, not the repository working
tree.

### Known Issues

- Patch preview still supports one explicit find/replace proposal against a
  project-local UTF-8 text file.
- Verification commands are limited to policy-allowed simple argv commands; no
  shell syntax, quoting, pipes, redirects, or environment expansion are
  supported.
- Model action output is not yet connected automatically to patch preview/apply.
- Verification output interpretation and final Korean task reporting remain
  future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.3.0 - Patch Diff Approval Preview

Release date: 2026-07-06

This release adds the first patch diff display and approval gate surface. It is
still a source-only developer preview: it does not ship model weights, external
plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato patch preview --path <path> --find <text> --replace <text>`
- Unified diff rendering for one explicit project-local text replacement
- Project-local proposal records under `.rpotato/patch-proposals/`
- Approval token display for the generated proposal
- `rpotato patch approve <proposal-id> --token <token> --dry-run`
- Approval gate verification and ledger event recording without patch application
- English and Korean documentation updates for the new patch boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (123 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path RELEASE_NOTES.md --find "Run Skeleton Preview" --replace "Run Skeleton Preview Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --dry-run`

The patch smoke returned `status: diff-ready`, displayed the expected unified
diff, then returned `status: gate-passed` for the dry-run approval. The target
file had no Git diff after the smoke, proving it was not modified.

### Known Issues

- Patch preview supports a single explicit find/replace proposal against a
  project-local UTF-8 text file.
- Patch approval is dry-run only in this release; it records the gate result but
  does not apply the patch.
- Agent-loop integration from model action to patch preview remains future work.
- Verification command execution, rollback handling, and final Korean reporting
  remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.2.0 - Run Skeleton Preview

Release date: 2026-07-06

This release adds the first `rpotato run` vertical slice on top of the managed
`llama.cpp` sidecar. It is still a source-only developer preview: it does not
ship model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- Context-aware `rpotato run "<task>"` skeleton
- Deterministic request routing into skill, mode, signals, and constraints
- Bounded repository context packing with source pointers
- Runtime-owned action candidate and next gate reporting
- Non-executing model action parsing from structured action lines or recognized action text
- Model/token/latency metrics written to the local SQLite observability projection
- Ledger events for intent, context pack, action candidate, model action, backend chat, and model run
- Source policy cleanup for versioned backend/model user agents
- English and Korean documentation updates for the new `run` boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

The latest Qwen3.5 smoke returned `model action parse: heuristic-text`,
`model action kind: patch-proposal`, `model action executable now: no`,
`guard: pass`, and `finish reason: stop`. This proves the current non-executing
runtime boundary and observability path, not patch quality or autonomous tool use.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest still includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still does not apply patches, execute commands, or treat model
  output as an approved action.
- Model action parsing is tolerant and non-executing; robust structured action
  generation and approval UI are future work.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.1.0 - Developer Preview

Release date: 2026-07-06

This is the first `rolling-potato` developer preview. It is a source-only
release tag for the early Rust runtime and CLI scaffold. It is not a stable
runtime contract and does not ship model weights, external plugin packages, or
prebuilt model/backend bundles.

### Included

- Rust CLI scaffold for `rpotato`
- Project/app state initialization
- Session list/new/resume projection backed by SQLite
- Runtime ledger and evidence validation surfaces
- Command/path policy checks and credential redaction
- Hook registry and fail-closed hook result validation
- Local plugin import/inspect/validate/enable/disable/remove surfaces
- Monitoring status, model summary, export, and dry-run prune surfaces
- Source-backed Qwen/Gemma model candidate manifest and evaluation gates
- Evaluation-only model artifact fetch with size and SHA-256 verification
- Managed `llama.cpp b9878` backend install/start/status/stop/health surfaces
- Non-streaming backend chat smoke path through `/v1/chat/completions`
- Qwen3.5 non-thinking smoke path with
  `chat_template_kwargs.enable_thinking=false`
- English docs plus Korean translations for the main project documents

### Verified In This Release

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

The Qwen chat smoke returned a clean Korean response through the managed
`llama.cpp` sidecar. This proves backend/model connectivity and the
non-thinking chat path, not full model quality.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still performs intent normalization only; the full agent loop is
  not implemented.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.
