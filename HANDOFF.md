# rolling-potato Handoff

## Repository

```text
/Users/sys/Desktop/codes/rolling-potato
```

- Remote: `https://github.com/MCprotein/rolling-potato.git`
- Latest release: `v0.29.1`
- CLI: `rpotato`
- Product: local-first coding-agent runtime for small local models

Read `AGENTS.md` before changing the repository. Continue safe, reversible work
without repeated confirmation. Commit and push meaningful units with
Conventional Commits. Release work must use `release/vX.Y.Z`, pass the release
policy and validation gates, merge to `main`, publish the matching tag/release,
and remove the merged release branch.

## Product Intent

`rolling-potato` is intended to replace Claude Code/Codex-style coding-agent
runtimes on low- and mid-range local machines. It is not a thin model wrapper or
only a model downloader.

The CLI and TUI are surfaces. The runtime core owns:

- deterministic intent and skill routing
- permissions, approvals, hooks, and stop gates
- session, transcript, workflow, ledger, evidence, and resume state
- ontology-backed context and source-pointer rereads
- model/backend lifecycle and resource governance
- token, latency, CPU, memory, disk, and benchmark monitoring
- plugin adapters, subagents, teams, and TUI state feeds

Model output never executes tools directly. Every side effect must pass runtime
policy, explicit approval where required, evidence recording, and verification.

## Current Release State

`v0.29.1` is the current complete release. GitHub Releases provides checksummed
binaries for:

- macOS Apple Silicon
- macOS Intel
- Linux x86_64
- Linux ARM64
- Windows x86_64

The release workflow runs the serialized Rust test gate, per-target build and
smoke checks, packages each binary, publishes per-asset checksums, and produces
an aggregate checksum file.

## Implemented Runtime Foundations

- Managed `llama.cpp` sidecar install, start, health, chat, stop, and resource
  lifecycle surfaces
- Source-backed model candidate manifests, evaluation fetch, checksum/size
  verification, benchmark planning, and a promotion/install gate
- Deterministic `run` routing, bounded repository context, source pointers,
  model response parsing, and non-executable model-action records
- Guarded patch preview, independent one-time patch/verification credentials,
  no-clobber apply, rollback record, and policy-allowed verification execution
- Canonical append-only runtime/project ledgers with a rebuildable SQLite
  observability projection for sessions, model
  runs, tokens, latency, resources, benchmarks, evidence, and team events
- Session history and session selection for resume
- Read-only TUI views for overview, monitoring, sessions, transcript events,
  approvals, diffs, evidence, and stop-gate state
- Resource governor, benchmark-driven optimization recommendations, and team
  admission/policy/ownership/dispatch preflight
- Canonical project-local typed ontology graph with compact context views and
  source-pointer reread rules
- Codex/Claude Code local-directory plugin import, hash/drift validation,
  capability mapping, and default-deny permission reporting

## Important Incomplete Boundaries

- `run` supports typed read-only completion and a bounded restart-safe patch
  workflow through separate apply/verification approvals, evidence, stop gate,
  and guarded Korean final reporting. General tool orchestration remains later.
- Model candidates are not defaults until source, license, artifact, backend,
  RAM/mmproj, and measured benchmark evidence passes the install gate.
- Runtime core resumes safe persisted phases of bounded patch workflows.
  Session `resume` still does not replay a durable conversation or continue a
  transcript-driven agent workflow.
- Hooks and skills expose validation/routing foundations but are not yet a full
  executable lifecycle state machine.
- TUI is read-only; it cannot approve, apply, resume, cancel, or mutate work.
- Team dispatch records policy and ownership decisions but does not launch real
  workers or advance/merge team stages.
- Imported plugins are inspected and validated but receive no execution
  authority yet.
- HTML monitoring and package-manager channels are intentionally later work.

## Next Versions

The version-only roadmap in `ROADMAP.md` is the source of truth. The immediate
sequence and non-skippable release gates are defined in
`docs/release-train.md`. The immediate sequence is:

1. `v0.30.0`: close model evaluation evidence and managed model install/default
   selection without predeclaring a winning Qwen/Gemma candidate.
2. `v0.31.0`: add backend response streaming and cancellation.
3. `v0.32.0`: implement durable transcript replay and real workflow resume.
4. `v0.33.0` onward: executable hooks/skills, interactive TUI, subagents,
   teams, and plugin execution adapters.

Package-manager distribution and an optional local HTML monitoring report come
after the runtime replacement path is operational.

## Model Evidence Boundary

Qwen and Gemma are evaluation candidates, not assumed defaults. Do not invent
or infer model names, licenses, performance, RAM fit, backend compatibility,
multimodal behavior, mmproj requirements, artifact URLs, sizes, or checksums.

Use these sources of truth:

- `docs/model-source-policy.md`
- `docs/model-manifest.md`
- `docs/model-licenses.md`
- `docs/model-eval.md`
- `docs/benchmarks.md`

Record a claim only with an explicit source or local measured evidence. Keep it
`unverified` otherwise. Model weights must never be committed or bundled into a
release artifact.

## Primary Reading Order

1. `README.md` or `README.ko.md`
2. `PLAN.md` or `docs/ko/PLAN.md`
3. `ROADMAP.md` or `docs/ko/ROADMAP.md`
4. `docs/runtime-architecture.md`
5. `docs/mvp.md`
6. `docs/ontology-runtime.md`
7. `docs/observability.md`
8. `docs/hooks.md`, `docs/skills.md`, `docs/subagents.md`,
   `docs/team-runtime.md`, `docs/tui.md`, and `docs/plugin-adapters.md`
9. `docs/model-source-policy.md`, `docs/model-manifest.md`,
   `docs/model-eval.md`, and `docs/benchmarks.md`

## Standing Product Constraints

- Korean user-facing final responses are required; code and unavoidable
  technical identifiers are handled separately by the output guard.
- External code contributions and external PRs are not accepted currently.
- Security reports and user issues may be accepted; maintainers own code and
  direction decisions.
- Plugin marketplaces, registries, catalogs, mirrors, and remote plugin URLs
  are out of scope. Plugin import is local-directory only.
- Foreign shell, MCP, background, remote connector, runtime-setting, and file
  write capabilities are blocked by default and require explicit policy and
  user approval before any supported execution path can use them.
- Runtime decisions must remain auditable through the ledger, SQLite
  projection, evidence records, and source pointers.
