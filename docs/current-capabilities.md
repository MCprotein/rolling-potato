# Current Capabilities

This document is the readable status map for the released
`rolling-potato v0.46.1` runtime. It groups the runtime by responsibility
instead of repeating one flat command list.

[README](../README.md) · [Documentation index](README.md) ·
[한국어](ko/current-capabilities.md)

> This is a capability guide, not a substitute for `rpotato --help`. The
> installed binary remains the source of truth for exact command syntax.

## Installation, First Run, and Updates (`v0.42.0`-`v0.46.1`)

The extracted GitHub Release binary can install or update itself in the
user-local CLI directory and register that directory in zsh, bash, fish, or
the Windows user PATH. Registration uses one owned block and is idempotent.
`init` repairs the registration when invoked through the installed binary;
shell-profile or environment-registration failures are reported without
blocking runtime-state initialization.

```sh
rpotato install
rpotato install --clean --dry-run
rpotato install --clean --yes
rpotato uninstall --clean --dry-run
rpotato uninstall --clean --yes
rpotato init
rpotato update --check
rpotato update
```

Standard install preserves config, models, backend assets, and project state.
Clean install removes only the global application-data root and the current
project's `.rpotato`, requires explicit confirmation, and is blocked while a
managed backend or generation is active. Dry-run reports the exact binary and
PATH-registration change as well as both deletion targets. Backend/generation
publication and deletion are serialized by one cross-process lease, and
process-liveness errors fail closed.

Clean uninstall removes the installed binary, the owned PATH registration,
global application data, and the current project's `.rpotato`. It preserves
the extracted invocation binary and source repository as user-owned files.
Windows self-deletion is scheduled for immediately after process exit.

At TUI startup, a six-hour cache and short request timeout check the official
latest stable GitHub Release without blocking offline startup. A newer version
is shown as a notice with `/update`; `rpotato update --check` performs the same
check explicitly. Applying an update is restricted to the owned managed
installation, downloads only the current platform's exact release archive,
verifies its matching SHA-256 sidecar, and stages only the exact binary entry.
Unix replacement is atomic; Windows replacement is scheduled after process
exit with rollback to the previous binary if the move fails.

The selected session/workflow `current-state` pointer is isolated under each
project's `.rpotato/state/` directory. A legacy installation-wide pointer is
migrated only for the matching project, and returning-project synchronization
requires the saved binding to be an ancestor of the canonical ledger. System
startup errors preserve their original message and exit code.

Read-only TUI views accept the valid upgrade shape of a legacy v1 ledger
prefix followed by a hash-chained v2 suffix. The legacy digest, physical chain,
head count, and bounded-read genesis remain verified without rewriting history.

## 1. Agent Loop and Context

The runtime can normalize a request, select an intent/skill route, pack bounded
repository context, call the backend, and parse the model result into a
runtime-owned typed action. Model text is never executed directly.

Representative entry points:

```sh
rpotato run "<request>"
rpotato intent classify "<request>"
rpotato intent routes
rpotato skill list
rpotato skill run <id> "<request>"
```

Current safeguards include one shared source budget for the active request and
resumed context, source-pointer evidence, policy checks, lifecycle hooks, and
a guarded Korean final report. Context compaction starts automatically at 75%
measured usage or manually through TUI `/compact`, targets 40% of the context
limit, and retains up to four recent transcript records. Deterministic typed
extraction remains available when the single bounded semantic rationale call
cannot run.

See [runtime architecture](runtime-architecture.md),
[command policy](command-policy.md), [hooks](hooks.md), and
[skills](skills.md).

## 2. Durable Sessions and Recovery

Canonical append-only ledgers own session and workflow history. SQLite is a
rebuildable projection, not resume authority. Durable transcripts keep
validated user, visible-model, tool, and evidence records while excluding
hidden reasoning and raw backend responses.

Representative entry points:

```sh
rpotato state
rpotato state reconcile
rpotato state resume
rpotato session list
rpotato session history
rpotato session new
rpotato session resume <session-id>
rpotato resume [<session-id>]
rpotato continue [<session-id>]
rpotato cancel
```

Recovery continues only a matching safe checkpoint. It does not automatically
repeat an uncertain backend request or verification command. Incremental
compaction checkpoints are immutable and hash-chained to the project, session,
previous checkpoint, and transcript boundary. Their fields are untrusted resume
hints; canonical transcripts, ledgers, instructions, and source artifacts remain
authoritative.

See [state lifecycle](state-lifecycle.md) and
[observability](observability.md).

## 3. Patch and Verification Workflow

The standalone preview surface creates a diff-only proposal. A patch produced
inside the agent workflow has separate apply and verification gates, source
hash checks, one-time credentials, and rollback records.

Representative entry points:

```sh
rpotato patch preview --path <path> --find <text> --replace <text>
rpotato patch approve <proposal-id> --token <token> --dry-run
rpotato patch approve <proposal-id> --token <token>
rpotato patch verify <proposal-id> --token <token>
rpotato patch token-rotate <proposal-id>
rpotato evidence validate <artifact-pointer>
```

A standalone preview cannot be approved or applied. Verification runs only the
pre-bound, policy-allowed command.

See [command policy](command-policy.md) and
[state lifecycle](state-lifecycle.md).

## 4. Backend Lifecycle

The managed `llama.cpp` path covers source-backed install planning, archive
verification, staged installation, process lifecycle, health checks, chat,
streaming, cancellation, and CPU/RSS/disk sampling.

The TUI prepares this path automatically. Granular diagnostic entry points are
available under the advanced namespace:

```sh
rpotato debug backend doctor
rpotato debug backend install-plan
rpotato debug backend status
rpotato debug backend start [--model <path>] [--ctx-size <tokens>]
rpotato debug backend stop
```

Sent model requests are not retried automatically. Raw prompt/response text is
not stored in monitoring records.

See [backend adapters](backend-adapters.md) and
[runtime architecture](runtime-architecture.md).

## 5. Models and Local Evidence

First-run setup and `/model` expose source-backed candidates and handle managed
download, verification, selection, and backend start. They show model/version,
quantization, download size, context limit, RAM status, license, and evidence
without presenting unmeasured RAM/capability claims as verified. Model weights
are downloaded to managed storage and are never committed to the repository.

Granular evaluation and promotion commands remain an advanced surface:

```sh
rpotato debug model list
rpotato debug model inspect <id>
rpotato debug model fetch-candidate <id> --for-evaluation
rpotato debug model benchmark-plan <id>
rpotato debug model promote <id> --evidence <file>
rpotato debug model install <id>
```

An explicit first-run selection may become the host's runtime default after the
pinned source, license, backend-compatibility source, artifact size, and SHA-256
revalidate. Its registry evidence remains `source-backed-manifest`; universal RAM
fit, capability, and benchmark claims remain unverified. The advanced
`model install`/promotion workflow keeps its stricter local evidence gate.

See [model source policy](model-source-policy.md),
[model manifest](model-manifest.md), [model evaluation](model-eval.md), and
[model licenses](model-licenses.md).

## 6. Hooks, Skills, and Plugin Adapters

Runtime-owned hooks and built-in skills execute inside the durable agent loop.
Local Codex/Claude Code-style plugin directories can be imported, inspected,
validated, enabled, disabled, and removed.

Representative entry points:

```sh
rpotato hooks list
rpotato hooks validate-result <json>
rpotato plugin import --from codex <local-path> --dry-run
rpotato plugin import --from claude-code <local-path> --dry-run
rpotato plugin list
rpotato plugin inspect <id>
rpotato plugin validate <id>
rpotato plugin enable <id>
rpotato plugin disable <id>
rpotato plugin remove <id> --keep-data
```

Imported instructions are untrusted prompt content. Enablement does not grant
shell, background, remote, sensitive-setting, or file-write authority.
Scripts, external hooks, MCP/app integrations, and remote plugin sources remain
blocked or unsupported.

See [plugin adapters](plugin-adapters.md), [hooks](hooks.md), and
[skills](skills.md).

## 7. Subagents and Teams

The runtime can run one bounded sequential child under an active parent
workflow. Team execution adds resource admission, declared lane ownership,
policy preflight, deterministic reconciliation, failure handling, and stop
gates.

Representative entry points:

```sh
rpotato subagent launch --role <role> --task <text> --tool <tool> --read <path>
rpotato subagent status [subagent-id]
rpotato subagent cancel <subagent-id>
rpotato team status
rpotato team admit --lanes <count>
rpotato team dispatch --lanes <count> --write-owner <lane:path>
rpotato team governor --lanes <count> --context-tokens <tokens>
```

Workers cannot directly execute commands, apply patches, start nested workers,
or bypass the parent approval boundary.

See [subagents](subagents.md) and [team runtime](team-runtime.md).

## 8. Monitoring and Benchmarks

The local append-only ledger and SQLite projection provide token, latency,
CPU, memory, disk, pressure, backend, model, session, benchmark, evidence, and
team metrics.

Representative entry points:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor export --format html > rpotato-monitor.html
rpotato monitor prune --before 30d --dry-run
rpotato benchmark validate <fixture.json>
rpotato benchmark record --fixture <fixture.json>
rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
rpotato benchmark report --format jsonl
```

The HTML export is a self-contained local file with no JavaScript, external
assets, network requests, or second telemetry source of truth. Benchmark
records marked `measured-locally` do not claim public benchmark parity.

See [observability](observability.md) and [benchmarks](benchmarks.md).

## 9. CLI and TUI Surfaces

No-argument `rpotato` starts the primary interactive line controller in a terminal and keeps a
read-only overview for non-terminal use. First run selects a source-backed model,
automatically prepares the managed backend, and starts the model without asking for
a GGUF path. The TUI exposes monitoring, sessions,
validated transcript/tool views, approvals, diffs, evidence, resume, and
cancellation without editing canonical state directly, and submits plain text as
agent requests. `rpotato tui` remains a compatibility alias.

The first frame uses a compact welcome that collapses to a one-line identity
header after the conversation starts. The focused bordered composer keeps a
semantic status line in `model | context used/limit | compaction | backend |
session` order. Korean and other wide-character turns wrap by terminal display
cells, and `/more` plus `/back` keeps every long-response line reachable.
Typing `/` opens the command palette before Enter; its entries share the same
registry as `/help`.
Representative public entry points are:

```sh
rpotato
rpotato init
rpotato doctor
rpotato run "<request>"
rpotato debug --help
```

See [TUI](tui.md), [CLI output style](cli-output-style.md), and
[DESIGN.md](../DESIGN.md).

## 10. Known Boundaries

- General unrestricted tool orchestration is not implemented.
- Only runtime-owned native hooks execute.
- Interactive-TUI source installation succeeds only on supported Unix paths;
  unsupported platform paths fail closed before mutation.
- Team workers return bounded evidence and non-executing patch proposals; team
  reconciliation does not apply worker-authored patches.
- Plugin scripts, agents, external hooks, MCP/LSP, background processes,
  remote connectors, and write grants do not receive execution authority.
- `monitor prune` is dry-run only.
- HTML monitoring is a local static export, not a server or remote dashboard.
- `v0.42.0` is limited to user-local installation, environment repair, clean
  reinstall, and clean uninstall; it does not add a package-manager channel.

The version history and next-version rule are in
[ROADMAP.md](../ROADMAP.md).
