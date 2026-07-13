# TUI Surface

TUI is a required product surface for a Claude Code/Codex replacement experience.

The first implementation can start from CLI commands, but the target runtime must support a terminal UI for interactive work.

The TUI design source of truth is [DESIGN.md](../DESIGN.md). Monitoring screens must also work on SSH/Linux servers, so they cannot assume a browser or GUI.

## Current Beta Surface

`v0.5.0` adds a dependency-free, read-only ASCII TUI beta:

- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

The beta reads existing runtime state and the SQLite observability projection. It shows project/session state, model/token summaries, session history, resume hints, and a clear read-only boundary. It does not approve, apply, resume, cancel, or mutate workflows.

`v0.6.0` extends the same read-only beta with patch proposal inspection:

- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`

The approval view reads project-local `.rpotato/patch-proposals/` records and shows proposal status, id, path, and replacement count. The diff view shows proposal metadata, approval/dry-run command hints, and the stored unified diff without applying or approving the patch.

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
