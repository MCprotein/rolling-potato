# rolling-potato Handoff

## Repository

```text
/Users/sys/Desktop/codes/rolling-potato
```

- Remote: `https://github.com/MCprotein/rolling-potato.git`
- Latest release: `v0.37.13`
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

`v0.37.13` is the current complete release. GitHub Releases provides checksummed
binaries for:

- macOS Apple Silicon
- macOS Intel
- Linux x86_64
- Linux ARM64
- Windows x86_64

The release workflow runs the serialized Rust test gate, per-target build and
smoke checks, packages each binary, publishes per-asset checksums, and produces
an aggregate checksum file. The active repository toolchain is pinned to Rust
1.97.0, current Node.js 24 GitHub Actions, current GA hosted runners, and
managed `llama.cpp b9982`; model measurements made on older pinned backends
remain historical evidence.

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
- Canonical durable user/visible-model/tool/evidence transcripts, rebuildable
  SQLite transcript projection, bounded source-pointer context reconstruction,
  and idempotent session/workflow `resume`/`continue`
- Read-only TUI views for overview, monitoring, sessions, durable transcript turns and events,
  approvals, diffs, evidence, and stop-gate state
- Resource governor, benchmark-driven optimization recommendations, and team
  admission/policy/ownership/dispatch preflight
- Canonical project-local typed ontology graph with compact context views and
  source-pointer reread rules
- Codex/Claude Code local-directory plugin import, hash/drift validation,
  capability mapping, and default-deny permission reporting
- Enabled canonical Codex instruction-only skill execution through source
  revalidation, native read-only hooks, evidence/stop gates, and restart-safe
  completion recovery
- Executable built-in skills and runtime-owned lifecycle hooks with durable
  state, deterministic ordering, policy enforcement, evidence, and stop gates
- Runtime-owned interactive TUI navigation and guarded approval, denial,
  resume, cancellation, diff, transcript, tool-output, and evidence operations
- One bounded sequential subagent per active parent with declared context,
  tools, write ownership, resource budgets, strict results, terminal failure
  handling, secret-safe persistence, and restart-safe parent evidence merge
- Runtime-owned team execution with exact manifest admission, resource-aware
  parallel/sequential lanes, action-time ownership, durable cancellation,
  interrupted-no-replay recovery, deterministic reconciliation, source-fresh
  evidence, and completion stop gates

## Important Incomplete Boundaries

- `run` supports typed read-only completion and a bounded restart-safe patch
  workflow through separate apply/verification approvals, evidence, stop gate,
  and guarded Korean final reporting. General tool orchestration remains later.
- Model candidates are not defaults until source, license, artifact, backend,
  RAM/mmproj, and measured benchmark evidence passes the install gate.
- Runtime core resumes safe persisted phases of bounded patch workflows and
  reconstructs durable bounded conversation/source context. It never
  automatically repeats an uncertain backend request or verification command.
- Only runtime-owned native hooks execute. Project/session/plugin hook
  executables remain disabled pending a separately reviewed loader and
  permission path.
- Approved source installation through the interactive TUI is supported on
  Unix; unsupported platform paths fail closed before target mutation.
- Team workers return bounded evidence and non-executing patch proposals. Team
  reconciliation does not apply worker-authored patches, and workers have no
  command, direct-write, nested-team, or nested-subagent authority.
- Enabled canonical Codex instruction-only skills can execute through the
  native read-only runtime after snapshot/frontmatter revalidation. Plugin
  scripts, hooks, MCP/app integrations, shell/background, remote, and write
  capabilities receive no execution authority.
- HTML monitoring and package-manager channels are intentionally later work.

## Next Versions

The version-only roadmap in `ROADMAP.md` is the source of truth. The immediate
sequence and non-skippable release gates are defined in
`docs/release-train.md`. The immediate sequence is:

1. `v0.37.1` through `v0.37.12` were unpublished implementation milestones
   consolidated into the exact-tree `v0.37.13` release. Every migration-ledger
   responsibility is complete and the `src` root contains only binary-owned
   `main.rs`.
2. `v0.38.0` begins only after the v0.37.13 tag, exact 11-asset set,
   checksums, packaged-binary smoke evidence, and release branch cleanup are
   verified. Its
   scope is Claude Code local plugin execution conformance through the
   established native adapter and default-deny permission boundary.
3. `v0.39.0` onward: integrated performance hardening followed by distribution
   and optional local monitoring surfaces.

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
4. `docs/code-architecture.md` or `docs/ko/code-architecture.md`
5. `docs/runtime-architecture.md`
6. `docs/mvp.md`
7. `docs/ontology-runtime.md`
8. `docs/observability.md`
9. `docs/hooks.md`, `docs/skills.md`, `docs/subagents.md`,
   `docs/team-runtime.md`, `docs/tui.md`, and `docs/plugin-adapters.md`
10. `docs/model-source-policy.md`, `docs/model-manifest.md`,
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
