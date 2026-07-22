# Design

## 1. Product Foundation

### Source Of Truth

- Status: Active
- Last refreshed: 2026-07-22
- Primary product surface: no-argument `rpotato` TUI; subcommand CLI for automation and diagnostics; optional local static HTML report
- Evidence reviewed:
  - `README.md`
  - `PLAN.md`
  - `docs/tui.md`
  - `docs/observability.md`
  - `docs/runtime-architecture.md`
  - `docs/architecture.md`
  - `docs/glossary.md`
  - `docs/benchmarks.md`
  - `PRIVACY.md`
  - `src/runtime_core/observability/monitor.rs`
  - `src/adapters/sqlite/observability_projection.rs`
  - user-provided Codex and Claude Code terminal references captured on 2026-07-22

### Brand

- Personality: small, fast, pragmatic local coding-agent runtime
- Trust signals: local-first execution, explicit approval, source-backed model claims, visible evidence, Korean final reporting
- Avoid:
  - overly decorative SaaS-dashboard visuals
  - language that makes the model seem magical
  - nested-card-heavy TUI layouts
  - color-dependent UI that breaks in SSH terminals

### Product Goals

- Goals:
  - Make `rpotato` enter the coding-agent TUI directly, with first-run setup and conversation in one terminal flow.
  - Provide a local agent-runtime experience that can replace Claude Code/Codex for practical workflows.
  - Let users inspect session state and monitoring even on low-end laptops, Linux servers, and SSH sessions.
  - Make token, latency, memory, guard, tool, and stop-gate metrics scannable by model.
  - Let users understand current bottlenecks and failure causes without digging through raw logs.
- Non-goals:
  - GUI desktop app in MVP
  - remote web dashboard by default in MVP
  - external telemetry for monitoring
  - raw prompt/source text stored in monitoring DB by default
- Success signals:
  - In an SSH terminal, a user can identify current model, token usage, latency, and failing gate within five seconds.
  - During a long agent run, pending approvals, active tool, subagent/team status, and model metrics remain visible in one coherent flow.
  - Benchmark results and real-run metrics use the same vocabulary.

### Personas And Jobs

- Primary personas:
  - Korean-speaking users
  - developers who find Claude Code/Codex subscription costs burdensome
  - users with 16 GB RAM class laptops
  - users running local model runtime on Linux servers or SSH sessions
  - users who are not comfortable with local LLM tooling but want coding-agent help
- User jobs:
  - inspect agent-session progress
  - compare token/latency/resource usage by model
  - inspect backend/model health
  - understand failed tool/guard/stop-gate causes
  - safely resolve pending approvals
  - compare benchmark and real-use results
- Key contexts of use:
  - narrow terminal pane
  - SSH Linux server
  - macOS/Windows local terminal
  - long-running coding-agent session
  - model benchmark or artifact audit

## 2. Experience System

### Information Architecture

- Primary navigation:
  - The default surface is one conversation transcript, not an operations dashboard.
  - The composer remains visible at the bottom and accepts natural-language coding requests.
  - Slash commands open model, status, session, doctor, and other secondary surfaces only when requested.
- Core routes/screens:
  - TUI default: compact welcome, transcript, composer, one runtime status line
  - TUI secondary: model picker, status summary, session history, diagnostics, approvals, evidence
  - CLI: `rpotato monitor status`, `rpotato monitor models`, `rpotato monitor session <id>`
  - optional local report: `rpotato monitor export --format html`
- Content hierarchy:
  1. current user and assistant turns
  2. active task progress, approval, or actionable failure
  3. composer
  4. compact model/context/backend/session status
  5. detailed metrics, hashes, ledger state, and logs only in explicit diagnostic views

### Design Principles

- SSH-first: every critical monitoring function must work in a plain terminal.
- Conversation first: the initial frame resembles a coding assistant, not a ledger viewer.
- Calm by default: do not show hashes, revisions, ledger counts, projection state, or raw workflow fields before the user asks for diagnostics.
- Progressive disclosure: transcript first, then drill down on model/session/tool/monitoring when requested.
- Evidence over confidence theater: completion and health claims cite metric/evidence state.
- Policy visibility: approval, privacy, redaction, and stop-gate status must be visible.
- Tradeoffs:
  - TUI cannot compete with HTML charts, so it should use compact tables, sparklines, sorted lists, and drill-down panels.
  - HTML can be better for offline reports, but it must not become the only monitoring surface.

### Visual Language

- Color:
  - Use restrained terminal colors with semantic meaning only.
  - Green: passing/healthy; yellow: degraded/waiting; red: blocked/failed; blue/cyan: selected/focus.
  - Do not rely on color alone; include status text or symbols.
- Typography:
  - Terminal-native monospace.
  - Short Korean labels by default in user-facing TUI.
  - Avoid long English headings in user-facing TUI.
- Spacing/layout rhythm:
  - The empty conversation uses one compact four-row welcome frame; it is replaced by a one-row identity header after the first turn.
  - Conversation turns use a consistent marker gutter and wrap by terminal display cells instead of discarding long text.
  - The composer is a persistent bordered focus area with one status row beneath it; status updates must not move the input row.
- Shape/radius/elevation:
  - Terminal borders are functional separators, not decorative cards.
  - Avoid nested boxes when spacing and headings are enough.
- Motion:
  - Minimal. Use periodic refresh and small progress indicators.
  - No animated noise in monitoring screens.
- Imagery/iconography:
  - No bitmap imagery in TUI.
  - ASCII/Unicode symbols may be used only when they improve scan speed and have text fallback.

## 3. Interface Contract

### Components

- Existing components to reuse:
  - CLI command output style from current scaffold
  - runtime status vocabulary from `docs/glossary.md`
  - observability metric groups from `docs/observability.md`
- New/changed components:
  - compact rounded-border welcome block that disappears once conversation starts
  - user and assistant turn presentation without diagnostic prefixes
  - bordered conversation composer with a persistent, semantically segmented runtime status line directly below it
  - first-run model picker and managed-backend setup flow
  - metric summary strip
  - model comparison table
  - session timeline
  - token budget meter
  - latency sparkline
  - gate/failure list
  - approval queue
  - log/evidence detail panel
  - export/prune dialog
- Variants and states:
  - healthy
  - degraded
  - blocked
  - collecting
  - stale
  - redacted
  - unavailable
- Token/component ownership:
  - Runtime core owns data state.
  - TUI owns presentation and user decisions.
  - `docs/observability.md` owns metric schema direction.

### Primary TUI Contract

- Running attached `rpotato` with no arguments opens the conversation controller.
- The first frame never renders the overview ledger page. It shows a compact welcome, the current project label, the composer, and the status line.
- Ordinary input appears as a user turn before dispatch. The visible result appears as an assistant turn; errors remain inline and state the direct cause and recovery action.
- Detailed revisions, hashes, ledger counts, projection freshness, workflow fields, and monitor tables are available only through explicit status/diagnostic views.
- On first run, the same terminal flow lists source-backed model choices and shows model ID/version, quantization, download size, context limit, RAM status, license, and recommendation evidence before confirmation.
- The managed backend is installed or reused automatically. The default path never asks the user for a `llama.cpp` executable or GGUF filesystem path.
- The composer remains the focus point. Its immediately following status line always uses this order: `model | ctx used/limit (%) | compaction | backend | session`.
- The attached-terminal composer uses a single rounded border, a cyan focus marker, and no placeholder text under the live cursor. The no-color/redirected fallback remains a plain `›` prompt.
- Status segments are colored independently: model/focus cyan, healthy green, due/degraded yellow, failed/stale red, and session/secondary labels muted. Never color the whole status row as one success state.
- The context segment shows measured usage and percentage; compaction remains adjacent to it when space permits. Narrow terminals truncate later segments rather than wrapping the status bar.
- Model and context values come from the latest recorded model run; backend state comes from the managed sidecar; session uses the active canonical session identity. Missing values and stale backend state are labeled, never invented.
- `/model`, `/compact`, `/update`, `/status`, `/sessions`, `/doctor`, `/more`, `/back`, `/clear`, `/help`, and `/quit` cover normal in-TUI operations. Existing granular subcommands remain an advanced compatibility surface under `rpotato debug --help`.
- Attached ANSI terminals may use semantic color and cursor positioning. Redirected output, `TERM=dumb`, and `NO_COLOR` remain plain, stable text.

### Accessibility

- Target standard: keyboard-first terminal accessibility with readable contrast and no color-only state.
- Keyboard/focus behavior:
  - Every action reachable without mouse.
  - Focus must be visible in monochrome terminals.
  - Destructive actions require explicit confirmation.
- Contrast/readability:
  - Avoid low-contrast dim text for critical values.
  - Preserve readability in light and dark terminal themes.
- Screen-reader semantics:
  - CLI monitor commands must provide plain text equivalents for TUI summary data.
- Reduced motion and sensory considerations:
  - Refresh interval should be configurable.
  - Do not flash on failures.

### Responsive Behavior

- Supported breakpoints/devices:
  - 80x24 minimum terminal target
  - wider terminal with multi-pane layout
  - narrow SSH pane with single-column fallback
- Layout adaptations:
  - Wide: left navigation, top metric strip, main table, detail panel.
  - Medium: top tabs, summary strip, one main panel, collapsible detail.
  - Narrow: stacked sections and explicit drill-down screens.
- Touch/hover differences:
  - No hover dependency.
  - Mouse support can be optional later, never required.

### Interaction States

- Loading:
  - Show data source, last update time, and whether SQLite projection or ledger replay is being read.
- Empty:
  - Explain that no model run has been recorded yet and show the next command.
- Error:
  - Show Korean cause, affected data source, and safe recovery command.
- Success:
  - Show verified metric timestamp and health status.
- Disabled:
  - Explain missing backend/model/session precondition.
- Offline/slow network:
  - Monitoring must work offline from local SQLite/ledger.

### Content Voice

- Tone: short, practical Korean for user-facing runtime/TUI copy
- Terminology:
  - `model run`
  - `token usage`
  - `context usage`
  - `backend health`
  - `stop gate`
  - `evidence`
  - `redacted`
- Microcopy rules:
  - Failure reasons should say what is blocked and why.
  - Privacy-sensitive panels should explicitly mark redacted data.
  - Do not use marketing copy inside monitoring screens.

## 4. Implementation Surfaces

### Implementation Constraints

- Framework/styling system:
  - The current interactive TUI is a std-only line controller.
  - The attached-terminal frame uses bounded ANSI layout to keep the status line below the composer while returning the cursor to the input line; scripted and redirected execution use a plain-text fallback.
  - A framework for a richer full-screen TUI is not selected.
  - SQLite projection access uses `rusqlite`.
  - TUI must consume runtime state through runtime core contracts, not direct DB ownership.
- Design-token constraints:
  - Semantic color names only: healthy, warning, failed, selected, muted.
  - Fixed-width columns need truncation rules.
- Performance constraints:
  - TUI must stay responsive while monitoring long-running sessions.
  - SQLite reads should be bounded and paginated.
  - Live updates should not block approvals.
- Compatibility constraints:
  - SSH/Linux-server use is a first-class context.
  - No browser requirement for core monitoring.
  - Optional HTML is generated locally from existing monitor query data and is not required for baseline operation.
- Test/screenshot expectations:
  - TUI smoke tests at 80x24 and wide terminal sizes.
  - The default-frame regression rejects raw ledger/hash/projection fields and proves composer/status ordering.
  - A natural-language greeting regression proves that it is rendered as a conversation and never starts a patch proposal.
  - Visual acceptance compares one 120x40 terminal capture against the 2026-07-22 Codex/Claude Code references; one bounded pass is sufficient unless the capture violates the contract.
  - HTML tests cover semantic structure, escaping, privacy markers, and narrow-screen layout without adding a browser runtime dependency.

### Monitoring TUI Screen Contract

Minimum overview layout:

```text
┌─ rolling-potato ─ Monitor ─────────────────────────────────────┐
│ model qwen…  backend healthy  tokens 12.4k  tps 18.2  mem 5.1G │
├─ Runs ────────────────┬─ Current Session ───────────────────────┤
│ model       tok  tps  │ workflow fix-test  gate waiting-evidence│
│ qwen-4b    12k  18.2 │ first token 820ms  retry 1  regen 0     │
│ gemma-e4b   9k  15.7 │ guard pass         tools 3/3             │
├─ Failures / Gates ────┴─ Detail ────────────────────────────────┤
│ ! missing test evidence      selected row details               │
├─ keys: 1 session 2 monitor 3 agents 4 evidence  e export q quit ┤
└─────────────────────────────────────────────────────────────────┘
```

Rules:

- The top strip always shows model, backend health, token total, throughput, and memory.
- Every metric has a timestamp or stale marker.
- Tables sort by the most operationally useful field by default: active run first, then failed/degraded, then recent.
- Detail panels must never show raw prompt/source by default.
- Export and prune actions must show dry-run summaries first.

### HTML Surface Position

HTML is an optional offline snapshot for reviewing and sharing local monitoring summaries. It does not replace the CLI or TUI and does not introduce a server.

Contract:

- TUI is the primary monitoring surface for local/SSH/server contexts.
- CLI monitor commands are the plain text fallback.
- `rpotato monitor export --format html` writes one complete HTML document to standard output so the user can redirect it to a file.
- HTML consumes the existing bounded monitor query data backed by the SQLite projection and canonical ledger. It must not create a separate monitoring truth source.
- The document is self-contained: no JavaScript, remote fonts, images, stylesheets, network requests, or local server.
- A restrictive content security policy blocks scripts, connections, forms, embedding, and base URL changes. Inline CSS is the only executable browser-adjacent content allowed.
- Every dynamic value is HTML-escaped. Raw prompt text, raw source text, credentials, and full local filesystem paths are never rendered.
- The report identifies its local data sources without exposing paths and shows the latest available metric timestamp or an explicit stale/unavailable marker.
- Semantic headings, landmarks, captions, and tables provide a readable document structure. Status meaning always has text and never depends on color alone.
- The layout supports light and dark color schemes. At narrow widths, sections stack and wide tables scroll horizontally without clipping the document.
- Empty, unavailable, redacted, and error states use short practical Korean copy and preserve the rest of the report.
- Export generation is read-only and offline. Opening the resulting file is an explicit user action.

## 5. Open Questions

- [ ] Should a richer full-screen TUI adopt a Rust framework?
- [ ] What is the default monitoring retention period?
