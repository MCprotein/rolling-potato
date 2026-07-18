# v0.29.0-v0.41.0 Release Train

This document is the durable execution contract for one-version-at-a-time Codex
Goal runs that implement and release the concrete versions in
[ROADMAP.md](../ROADMAP.md) from `v0.29.0` through `v0.41.0`. Ultragoal is not
used for this release train.

## Objective

Complete the Claude Code/Codex replacement path as a local-first coding-agent
runtime for small local models. Execute one minor-version release cycle at a
time. Do not skip a version, declare unmeasured model results, or move packaging
polish ahead of runtime correctness.

Each version uses a separate Goal and completes only after its tag, GitHub
Release, required assets, checksums, workflow gates, branch cleanup, and review
evidence are verified. The next version starts with a new Goal.

The unpublished `v0.37.1` through `v0.37.12` rows are implementation
milestones, not separately shipped artifacts. The maintainer-approved
whole-train squash merged their exact final tree as the `v0.37.13` candidate.
Publication therefore uses one `v0.37.13` release branch and tag; creating
multiple tags with the same completed implementation would misrepresent the
artifact history. This consolidation is specific to the already-completed
v0.37.x architecture train and does not weaken the one-version release cycle
for later roadmap versions.

## Required Version Cycle

Every version must complete this sequence:

1. Start `release/vX.Y.Z` from current `main`.
2. Confirm the version objective and acceptance evidence against the roadmap and
   this document.
3. Implement the smallest complete version scope with tests.
4. Update English and Korean documentation together.
5. Run targeted tests, `cargo fmt --check`, `cargo test --locked`,
   `cargo clippy --all-targets -- -D warnings`, release build, and relevant CLI
   smoke tests.
6. Run an independent review and resolve all blocking findings.
7. Commit and push the release branch with Conventional Commits.
8. Run `scripts/release/verify-release-policy.sh` and the release checklist.
9. Merge to `main`, reverify the merged commit, tag `vX.Y.Z`, and publish the
   GitHub Release.
10. Verify the GitHub Actions conclusion, every required platform archive,
    per-asset checksums, aggregate checksums, and packaged-binary smoke results.
11. Remove the merged local/remote release branch and complete the current
    version Goal with concrete evidence before starting the next version Goal.

If a published tag cannot satisfy the release asset gate, create the smallest
patch recovery release. Mark the failed tag superseded only with recorded
evidence, and do not start the next minor version until the recovery release is
complete.

## Version Evidence

| Version | Required completion evidence |
| --- | --- |
| `v0.29.0` | Restart-safe `run` workflow covering persisted typed action, ontology-backed source reread, separate patch/verification approvals, guarded apply, verification interpretation, canonical ledger authority, stop gate, evidence, and guarded Korean final report |
| `v0.30.0` | Source-backed candidate evaluation, actual local backend/RAM/mmproj/benchmark records, install-gate decision, and managed install/default flow only for a candidate that passes |
| `v0.31.0` | Streaming response and separate-process cancellation tests, a 1-300,000 ms total timeout, zero retries after request send, no stale generation lease or terminated sidecar on interruption, and lifecycle/resource/model-run evidence with unknown incomplete usage preserved |
| `v0.32.0` | Process-restart test that resumes a selected session, rebuilds bounded context from durable transcript/source pointers, and continues the interrupted workflow idempotently |
| `v0.33.0` | Hook ordering/fail-closed fixtures and executable built-in skill state-machine tests proving policy, evidence, and stop criteria cannot be bypassed |
| `v0.34.0` | Interactive terminal tests for approve/deny, diff/tool output, resume/cancel, and monitoring/session operations through runtime-owned state |
| `v0.35.0` | Bounded subagent launch, scoped context/tool/write/resource enforcement, structured result, failure handling, and parent evidence merge tests |
| `v0.36.0` | Real team lane dispatch/stage/reconciliation tests covering parallel and sequential modes, action-time ownership, failed lanes, resource pressure, verification, and stop gates |
| `v0.37.0` | Codex local plugin capability execution tests through native adapters, with risky capabilities blocked until explicit approval and no marketplace/remote source path |
| `v0.37.1` | Bilingual code-architecture contract, recursively complete file-and-slice migration ledger, private compile-connected skeleton, dependency contract test, and exact-head read-only candidate CI |
| `v0.37.2` | Private foundation/filesystem/terminal owners, no completed-slice compatibility facades, consolidated platform contracts, targeted owner tests, and exact-head candidate CI |
| `v0.37.3`-`v0.37.13+` | Targeted behavior locks and migration evidence for each remaining ledger slice, byte/order/recovery/security compatibility, one bounded review per stabilized patch, and zero compatibility facades before v0.38.0 |
| `v0.38.0` | Claude Code local plugin mapping/conformance tests, explicit unsupported-semantics reporting, and the same default-deny boundary |
| `v0.39.0` | Measured end-to-end agent/subagent/team CPU, RSS, context, token, latency, and throughput evidence plus regression fixtures for confirmed failures |
| `v0.40.0` | Current official package-format validation and clean install/upgrade/uninstall tests for the adopted Homebrew/Scoop channels against GitHub Release checksums |
| `v0.41.0` | Local-only static HTML export tests proving SQLite/ledger parity, redaction, no external telemetry, no second source of truth, and usable desktop/mobile browser rendering |

## Non-Skippable Gates

### Model And Benchmark Evidence

- A model candidate remains `unverified` until source, license, artifact URL,
  checksum, size, backend compatibility, RAM fit, mmproj need, and measured
  product benchmark evidence are recorded.
- Public benchmark comparison requires the same dataset/version, prompt/template,
  backend, quantization, context, sampling, and scoring conditions. Otherwise the
  result must be labeled non-comparable.
- Never invent a score, capability, default model, or hardware requirement.
- If required local hardware, storage, network, or upstream artifacts are not
  available, checkpoint a blocker. Do not substitute an inferred result.

### Runtime Safety And Persistence

- Model output never executes a tool directly.
- File writes, commands, downloads, plugin capabilities, subagents, and teams
  must pass runtime policy and explicit approval where required.
- Pending approvals, actions, evidence, and resume state must survive process
  restart before a workflow is considered complete.
- Unknown, corrupt, stale, or conflicting state fails closed and records the
  validation gap.
- Hook, skill, plugin, TUI, subagent, team, benchmark, and HTML surfaces cannot
  create alternate policy, state, telemetry, or stop-gate authorities.

### Plugin Boundary

- Plugin import remains local-directory only. Marketplace, registry, catalog,
  mirror, and remote URL integration stay out of scope.
- Shell, MCP, background process, remote connector, runtime-setting, and file
  write capabilities remain blocked by default.
- Codex compatibility is implemented before Claude Code compatibility.
- Unsupported foreign semantics are reported; they are not silently emulated
  with wider permissions.

### Release And Distribution

- A version is not complete at local test success or tag creation. The release
  workflow and required platform assets must complete successfully.
- Required targets remain macOS Apple Silicon, macOS Intel, Linux x86_64, Linux
  ARM64, and Windows x86_64 unless an evidence-backed roadmap change is accepted.
- Package-manager implementation must be checked against current official
  specifications at implementation time.
- Winget is not an adopted roadmap or publication channel. Historical v0.40.0
  generator and validation artifacts do not authorize or require an upstream
  submission.
- Model weights, local state, logs, secrets, `.omx/`, `.rpotato/`, and build
  outputs must not enter commits or release archives.

### Documentation And Claims

- English base documentation and Korean translations change together whenever
  user-facing behavior or architecture contracts change.
- Model, license, performance, compatibility, and legal claims require explicit
  sources. Unresolved claims stay clearly unverified.
- Release notes must describe measured behavior and known boundaries without
  implying unfinished replacement capabilities are complete.

## Blocker And Steering Rules

Do not advance to the next version when any required gate fails. Keep the current
story active or record a blocker story when there is:

- missing or conflicting model/license/artifact evidence
- insufficient hardware, disk, or network for required real measurement
- failed tests, independent review, architecture invariant, or security boundary
- incomplete GitHub Actions run or missing/corrupt release assets
- an upstream/package specification change that invalidates the implementation
- a destructive, credential-gated, license-changing, or materially scope-changing
  decision that lacks user authority

Per-version Goal execution may split work, add blocker-resolution tasks, reorder
only pending work when evidence proves it necessary, or revise pending wording.
It must not weaken a gate, fabricate completion, silently skip a version, or
change the product objective.

## Final Quality Gate

Before marking a version Goal complete:

1. Verify the version tag, GitHub Release, workflow, assets, checksums, and branch cleanup.
2. Run a changed-file cleanup and consistency pass.
3. Rerun the full verification suite.
4. Prove the architecture invariants from this document, `PLAN.md`,
   `ROADMAP.md`, and the runtime architecture documents.
5. Obtain independent `code-reviewer` approval; require an additional
   `architect` review for architecture-changing versions and the final `v0.41.0`
   train closeout.
6. Record the final quality evidence and only then complete that version's Codex Goal.
