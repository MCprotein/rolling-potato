# Runtime Architecture

The product body of `rolling-potato` is the coding-agent runtime, not the CLI. The CLI is the first MVP user surface.

The target is a local agent runtime that can replace Claude Code/Codex for practical workflows. Hooks, skills, subagents, team runtime, and TUI are first-class runtime capabilities. Claude Code/Codex-style plugins are converted into runtime capabilities through adapters instead of being executed directly.

## Layers

```text
User
  -> Surface
     -> Runtime core
        -> Backend adapter
           -> Local inference backend
              -> Model artifact
```

## Surface

Surfaces are entrypoints into the runtime.

MVP surfaces:

- `rpotato` CLI
- `rpotato` TUI after the first CLI vertical slice

Potential later surfaces:

- IDE extension
- local HTTP control API
- benchmark harness

Surface-owned responsibilities:

- command parsing
- user input capture
- approval prompt rendering
- progress display
- diff/result display
- final report display
- subagent/team status display
- evidence and stop-gate display

Not surface-owned:

- tool permission decisions
- model/backend artifact trust decisions
- context selection policy
- ontology merge and graph-store updates
- patch application
- stop/completion decisions

## Runtime Core

The runtime core owns the parts that matter most in Claude Code, Codex, and Gaja Code-style agent experiences.

Runtime core responsibilities:

- session lifecycle
- session history query and resume selection
- runtime state
- append-only ledger
- observability projection
- ontology graph store and query projection
- hook lifecycle
- skill registry and invocation
- plugin import, validation, and enablement
- model manifest validation
- backend lifecycle
- repository indexing
- ontology lifecycle
- context packing
- prompt/action compilation
- agent loop
- subagent lifecycle
- team coordination
- tool execution policy
- patch generation and application
- verification command classification
- evidence collection
- stop gate
- token/resource monitoring
- Korean output guard

Rule: model output is not tool-execution authority. The runtime core interprets model output, then executes only action candidates that pass policy gates.

## Backend Adapter

Backend adapters form the boundary between runtime core and inference backend.

MVP adapter:

- `llama.cpp` sidecar

Required adapter capabilities:

- health check
- model metadata
- context length reporting
- chat completion
- streaming tokens
- cancellation
- backend diagnostics

Adapters do not know about project files, user approvals, patches, or command policy. Those boundaries remain in the runtime core.

## Plugin Adapter

Plugin adapters are the compatibility boundary that converts foreign runtime plugin packages into `rpotato` capabilities.

Required capabilities:

- source runtime detection
- source manifest parsing
- local path canonicalization and remote source rejection
- normalized `rpotato` plugin manifest generation
- skill/hook/subagent/MCP capability mapping
- unsupported capability report
- permission report
- enable/disable state

Adapters do not execute foreign plugins directly. The Codex source-runtime adapter is implemented first; the Claude Code adapter follows. External marketplaces, registries, catalogs, mirrors, and remote URL sources are not accepted. Execution is possible only after converted capabilities pass runtime hooks, tool policy, ledger, and evidence gates.

See [plugin-adapters.md](plugin-adapters.md).

## Session History And Resume

Session resume is a runtime-core responsibility, not a CLI-only shortcut.

The runtime keeps three separate layers:

- append-only ledger: audit source for session events
- SQLite projection: queryable session history for CLI/TUI selection
- current state: the currently selected `session_id` and resume metadata

`rpotato session list`, `rpotato session history`, and bare `rpotato resume` read selectable history from SQLite. `rpotato session resume <session-id>` and `rpotato resume <session-id>` write the selected session id into current state so later commands append under the selected session identity. The later agent-loop phase uses that selected session to replay transcript, rebuild context, and continue conversation.

## Model Artifact

Model artifacts are not owned by the runtime. They are third-party artifacts that preserve their original source and license.

The runtime core blocks model install until the following are confirmed:

- upstream source
- artifact provider
- artifact URL
- license
- file size
- SHA-256
- backend compatibility
- RAM-fit evidence for product default selection

Current Qwen/Gemma GGUF candidates have source-recorded URLs, file sizes, and expected SHA-256 values, but they remain `unverified` until local `llama.cpp b9878` smoke, RAM-fit measurement, and mmproj-need review are complete.

## Control Flow

Default flow for `rpotato run "테스트 실패 고쳐줘"`:

1. CLI surface forwards the user request to the runtime core.
2. Runtime core resolves matching skill and mode.
3. Runtime core opens project boundary and state.
4. Runtime core initializes the hook pipeline.
5. Runtime core queries Layer A repo facts and Layer B ontology from the canonical graph store/projection.
6. Runtime core promotes required source pointers to original-file reads.
7. Runtime core creates a bounded subagent or team stage when needed.
8. Runtime core sends a constrained prompt/action request to the model/backend adapter.
9. Runtime core interprets model output as an action candidate.
10. Runtime core applies permission policy and evidence gates.
11. CLI/TUI surface displays approval prompts or diffs when needed.
12. Runtime core executes only approved actions.
13. Runtime core records verification results and evidence in the ledger.
14. Runtime core records token, latency, backend, guard, tool, and ontology-query metrics in the local SQLite projection.
15. Stop gate decides completion.
16. Reporter output passes the Korean output guard before the surface displays it.

### Durable Patch Workflow (v0.29.0)

The `run` patch path uses immutable versioned workflow snapshots plus an atomic
committed-revision pointer as its canonical artifact, with matching append-only
ledger checkpoints as audit authority. Every revision carries a schema version,
monotonic revision, previous hash, and artifact hash. A synced transaction record
recovers interrupted checkpoint windows. Missing, corrupt, stale-project,
hash-conflicting, multi-active, malformed-ledger, or ledger-unmatched state fails
closed. SQLite is only a rebuildable projection.

Model output is stored as a non-executable action. The runtime rereads the named
source before proposal, approval, apply, and stop-gate evaluation. Approval binds
workflow/action/proposal IDs, before/after hashes, and the exact verification plan,
and is persisted before file writes. The runtime issues an OS-CSPRNG nonce once,
stores only its hash, and cannot reconstruct it through state or TUI views. Pending
approval resumes without another backend call. Apply and rollback first atomically
move the destination to a unique guard, verify those moved bytes, and install only
into a still-nonexistent destination. A concurrent editor therefore causes a
no-clobber conflict instead of an overwrite, and a synced transaction supports
recovery. Rollback verifies the saved original-byte hash and reports failure
truthfully. Completion, including resume
from `complete`, requires fresh applied source and passing evidence before
deterministic Korean reporting.

Patch verification never invokes a shell. One strict argv parser is shared by
classification and execution and permits only `pwd`, narrowly scoped `cargo
test|check|clippy`, and exactly `cargo fmt -- --check` for the current crate. Bare
or `--all` formatting is rejected. Interpreters, path-like
executables, metacharacters, chaining, workspace selection, and external
manifest/package selection fail closed. A durable `verification-started`
checkpoint is written before process spawn; an inconclusive restart never reruns
that command automatically.

The approval nonce may be recovered only by explicitly running `rpotato patch
token-rotate <proposal-id>` while the canonical workflow remains pending. Rotation
uses a new OS-CSPRNG nonce, checkpoints its hash in a new workflow revision, and
invalidates the old nonce; state and TUI views cannot reconstruct either secret.
Standalone preview is diff-only and cannot be approved, applied, or verified.
Legacy v2 plaintext credentials are atomically rewritten to hash-only form before
the record is blocked; a new canonical workflow preview is then required.

The workflow retains required project-local raw material in mode-0600 restricted
artifacts until project cleanup: workflow find/replace snippets and source
pointers, proposal diff and proposed source bytes, guarded transaction paths and
hashes, and `.rpotato/patch-proposals/*.rollback` original bytes. These artifacts
serve proposal/apply/recovery/rollback only. Raw source bytes are never copied into
SQLite, monitor views, ledger details, or verification evidence.

## Non-Negotiable Boundaries

- CLI surface does not bypass runtime policy.
- TUI surface does not bypass runtime policy.
- Backend adapters do not write files or execute commands directly.
- Plugin adapters do not execute foreign plugin code directly.
- Model output does not become a shell command or patch automatically.
- Hooks can narrow behavior but cannot widen permissions.
- Skills declare requirements but do not execute tools directly.
- Subagents and teams inherit parent runtime policy.
- Snippets are not authoritative source.
- Ontology claims are not confirmed without source references and confidence.
- Stop is decided by evidence gates, not by the model.
- Monitoring is local-first runtime state, not external telemetry.
