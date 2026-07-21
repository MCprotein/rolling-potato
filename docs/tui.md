# TUI Surface

The TUI is the default product surface for a Claude Code/Codex replacement experience.

Users enter it by running `rpotato` with no arguments. `rpotato tui` and
`rpotato tui interactive` remain compatibility aliases for automation and tests,
not the primary usage. Plain text is an agent coding request, never a shell command.

The TUI design source of truth is [DESIGN.md](../DESIGN.md). Monitoring screens must also work on SSH/Linux servers, so they cannot assume a browser or GUI.

## Current Surface

`v0.5.0` adds a dependency-free, read-only ASCII TUI beta:

- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

The beta reads existing runtime state and the SQLite observability projection. It shows project/session state, model/token summaries, session history, resume hints, and a clear read-only boundary. It does not approve, apply, resume, cancel, or mutate workflows.

`v0.6.0` extends the same read-only beta with patch proposal inspection:

- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`

The original approval view read project-local `.rpotato/patch-proposals/` records. As of v0.34.0, both one-shot commands delegate to the same bounded canonical runtime facade as the interactive controller: approvals show only canonical team-admission events and the active workflow-bound proposal, while diff rejects unbound or oversized directory-only records without applying or approving a patch.

`v0.7.0` extends the read-only beta with selected-session event inspection:

- `rpotato tui transcript <session-id>`

As of v0.32.0, the transcript view validates canonical durable user/visible-model/tool/evidence artifacts and shows those turns beside the ledger-ordered event timeline. It remains read-only and excludes hidden model responses, source-file bodies, patch fragments, verification-command text, and raw event details.

`v0.8.0` extends the read-only beta with evidence and stop-gate status inspection:

- `rpotato tui evidence`

The evidence view reads the runtime evidence JSONL path, project evidence directory, SQLite `evidence_records` count, SQLite `stop_gate_results` count, and stale policy summary. It intentionally does not pass or fail workflows; terminal stop-gate evaluation remains runtime-core work.

`v0.10.0` extends the read-only monitor view with resource pressure and token throughput:

- `rpotato tui monitor`

The monitor view reads SQLite model summaries and the latest `resource_samples` row. It shows model run counts, token records, average latency, average tokens per second, resource sample count, latest pressure status, CPU percent, average/peak RSS, disk bytes, and recorded timestamp. It remains read-only; export, prune, and governor behavior stay outside the TUI beta.

This is the framework decision for the first beta: keep the initial surface dependency-free and terminal-safe, then decide later whether a richer TUI crate is justified after interaction requirements stabilize.

`v0.34.0` promotes the terminal surface from read-only inspection to a runtime-owned,
line-oriented interactive controller without adding a dependency:

- `rpotato tui` starts the controller when both input and output are attached to a
  terminal; redirected use keeps the read-only overview.
- `rpotato tui interactive` starts the same controller explicitly and also supports
  deterministic piped-input tests.
- `view`, `next`, `prev`, and `select <canonical-id>` navigate canonical runtime
  state. `select session <session-id>` confirms and dispatches a canonical session
  selection through the runtime lease boundary. Input that does not match a reserved
  TUI command is submitted as an agent request and is never executed as a shell command.
- `view tool-output <artifact-id>` opens a ledger-bound, owner/path/hash-validated,
  size-bounded sanitized tool artifact. Session and transcript pages use the canonical
  ledger and durable artifacts; SQLite is never their authority.
- `approve <proposal>`, `approve verification <proposal>`, `deny`, `resume`, and
  `cancel` require a selected workflow, a fresh runtime selection lease, and an
  explicit `yes` confirmation. Credentials are read once with terminal echo disabled;
  SIGINT/SIGTERM and Windows console termination restore the captured input mode before
  process termination.

The current default-entry contract is:

- Attached no-argument `rpotato` starts the controller.
- Redirected no-argument execution prints the read-only overview and exits.
- Plain text that does not match a reserved TUI command is submitted to the agent runtime.
- Shell-looking input is not executed directly and still crosses model/runtime policy boundaries.
- First-run backend/model selection and installation run inside this TUI. The default
  flow does not require a `llama.cpp` executable or GGUF path.

### First Run

When no default model is configured, `rpotato` and attached `rpotato init` open the
setup flow before conversation:

1. Show source-backed choices with model ID/version, quantization, download size,
   context limit, RAM status, license, and evidence note.
2. Accept a list number or exact model ID and request explicit download confirmation.
3. Install or reuse the pinned managed backend.
4. Download the selected artifact, verify size and SHA-256, register the explicit
   user selection, and start it with the default context size.

RAM suitability and unmeasured capability remain labeled `unverified`; setup does not
turn a source manifest into benchmark evidence. `/model` lists the same catalog, and
`/model <id>` switches models through the same managed path.

### Composer Status Line

On an attached ANSI terminal, the composer is followed by one stable status line and
the cursor returns to the input row:

```text
request> _
model gemma-3n-E4B-it-Q4_K_M | ctx 812/4096 (19%) | backend ready | session 01J…
```

The fields always stay in `model | context | backend | session` order. They come from
the latest model-run projection, managed backend sidecar, and active canonical session;
missing values and stale backend state are displayed explicitly. `NO_COLOR`, `TERM=dumb`, redirected,
and scripted execution use plain text without ANSI control sequences.

Normal interactive commands are `/model`, `/status`, `/sessions`, `/doctor`, `/clear`,
`/help`, and `/quit`. Granular backend, registry, benchmark, policy, and inspection
commands remain available for diagnostics under `rpotato debug --help`.

<!-- TUI-READ-CONTRACT:START -->
The eight views (`overview`, `monitor`, `sessions`, `transcript`, `tool-output`,
`approvals`, `diff`, and `evidence`) use view-specific item, byte, scan, line, and
pagination bounds. Every page carries canonical current/workflow revision and hash,
ledger sequence and hash, relevant content or transcript hash, projection watermark,
validation time, and one typed continuation: `complete`, `next-page`, `truncated`,
`unavailable`, or `redacted`. SQLite is a derived metrics/freshness projection only;
freshness is exactly `fresh`, `stale`, `projection-lag`, or `unavailable`. Read paths do
not acquire mutation leases, repair state, write validation gaps, or admit corrupt,
unbound, SQLite-only, or directory-scan-only candidates.
<!-- TUI-READ-CONTRACT:END -->

The runtime owns all mutation, intent IDs, immutable receipts, and the closed 27-row
outcome table. A successful patch approval commits one exact prepared bundle containing
11 ordered members and the exact E0-E9 semantic event chain. Restart recovery replays
only idempotent stored effects, never downgrades an installed R+2 workflow pointer, and
returns a refresh-only receipt for a repeated committed intent without re-displaying a
secret. The first successful approval writes the new verification credential to the
terminal exactly once and never stores it in the next rendered notice. Read-facing
commands do not create new product mutations, but command startup may finish an
already-committed transition journal or rebuild a lagging derived projection. Project
ledger, operation log, and SQLite are derived in that order; a failed
projection preserves the journal and an exact E9 lag marker until repair converges.

Terminal output escapes ANSI/OSC and control bytes, applies width/height-aware bounded
rendering, and distinguishes frame failure before dispatch from failure after commit so
the latter is never retried as a new mutation. Tool output is restricted to the current
project's canonical ledger events; approval and diff views expose only the active
workflow's bounded, workflow/action/hash-bound proposal.

Known v0.34.0 limits:

- Approved source installation succeeds only on Unix. Unsupported platforms block
  before journal commitment and before any source effect.
- Interaction is line-oriented, not a raw-key or full-screen terminal protocol.
- A concurrent external writer that starts after the final pathname validation and
  wins the validate-to-unlink race is outside the supported guarantee. Observable
  conflicts fail closed; the unobservable interval is not claimed to be atomic.

## Goals

- Make long-running agent sessions inspectable.
- Show runtime state without requiring users to read raw logs.
- Support approvals, diffs, tool output, subagents, and teams.
- Support plugin import and permission review.
- Make context, evidence, and stop-gate state visible.
- Make model/token/resource monitoring available in terminal-only environments.
- Preserve a keyboard-first terminal workflow.

## Non-Goals

- GUI desktop app
- web dashboard as the primary interface
- TUI-owned policy
- direct runtime-core bypass
- raw prompt/source exposure by default in monitoring screens

## Required Views

Minimum TUI views:

- chat/session transcript
- current plan
- context and ontology summary
- pending approvals
- diff viewer
- tool output viewer
- model/backend status
- model/token usage summary
- CPU/memory/resource-pressure summary
- subagent status
- team status and latest team runtime event
- plugin permission review
- evidence/stop-gate status
- logs and diagnostics

## Interaction Model

TUI actions:

- submit user request
- approve or deny tool call
- approve or deny patch
- approve or deny command
- enable or disable plugin
- approve or deny blocked plugin capability per capability
- inspect source pointer
- inspect evidence
- switch active view
- open session history
- resume selected session
- cancel workflow
- resume workflow

Every action passes through the runtime core.

## Layout Direction

Initial layout:

```text
┌────────────────────────────────────────────┐
│ transcript / active task                   │
├───────────────┬────────────────────────────┤
│ plan/context  │ diff/tool/evidence detail  │
├───────────────┴────────────────────────────┤
│ approvals / status / command bar           │
└────────────────────────────────────────────┘
```

Monitoring layout:

```text
┌─ Monitor ──────────────────────────────────┐
│ model/backend  tokens  tps  latency  mem   │
├───────────────┬────────────────────────────┤
│ model runs    │ selected session detail    │
│ failures      │ gate/tool/evidence status  │
├───────────────┴────────────────────────────┤
│ export / prune / refresh / command bar     │
└────────────────────────────────────────────┘
```

Monitoring UI rules:

- overview first, drill-down second
- active/degraded/blocked runs sort first
- every metric shows a timestamp or stale marker
- no color-only status; include text status
- no raw prompt/source by default
- export and prune actions require dry-run summaries
- narrow terminals fall back to stacked single-panel views

## Runtime Contract

TUI consumes runtime state:

- session status
- session history
- active workflow
- active skill
- active subagents
- active team stage
- pending approvals
- plugin capability and permission report
- ledger tail
- evidence status
- backend/model status
- token/resource metric summary
- metric freshness/staleness state

TUI emits user decisions:

- request
- approve
- deny
- select session
- cancel
- resume
- inspect

## Command Palette Routing

Phase 3 fixed command-palette routing:

- `request.submit` -> `rpotato run <request>`
- `intent.preview` -> `rpotato intent classify <request>`
- `skill.run` -> `rpotato skill run <id> "<request>"`
- `plugin.review` -> `rpotato plugin inspect <id>` or `rpotato plugin validate <id>`
- `plugin.toggle` -> `rpotato plugin enable <id>` or `rpotato plugin disable <id>`
- `workflow.cancel` -> `rpotato cancel`
- `session.history` -> `rpotato session list`
- `session.resume` -> `rpotato resume <session-id>`
- `workflow.resume` -> `rpotato state resume`
- `monitor.open` -> `rpotato monitor status`
- `evidence.inspect` -> `rpotato evidence validate <artifact-pointer>`

Current state owns active workflow. TUI actions emit only requests to the runtime core; skills, plugins, subagents, and teams cannot create independent workflows without a parent workflow pointer.

## Accessibility And Constraints

- Korean user-facing labels by default
- readable at small terminal sizes
- SSH/Linux-server environment is first-class
- no hidden destructive shortcuts
- keyboard-first
- terminal resize handling
- clear fail-closed error display

## Validation

TUI needs smoke tests for:

- small terminal layout render
- approval flow does not bypass runtime policy
- diff view handles long files
- cancellation updates runtime state
- team/subagent status updates
- model/token/resource monitoring view updates
- plugin permission review does not bypass runtime policy
- shell/bin/MCP/background/remote/file-write capabilities are shown as blocked by default
- Korean output guard is visible in final report
