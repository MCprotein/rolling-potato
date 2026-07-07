# Release Notes

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
- Runtime resource governor behavior remains planned for v0.11.0+.
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
- Runtime resource governor behavior remains planned for v0.11.0+.
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
