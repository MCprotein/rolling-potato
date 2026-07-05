# Design

## Source of truth

- Status: Draft
- Last refreshed: 2026-06-29
- Primary product surfaces: CLI, TUI, later optional local HTML report/dashboard
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

## Brand

- Personality: 작고 빠르며 실용적인 로컬 코딩 에이전트 runtime
- Trust signals: local-first, explicit approval, source-backed model claims, visible evidence, Korean final reporting
- Avoid:
  - SaaS dashboard처럼 과하게 장식적인 화면
  - 모델을 마법처럼 보이게 하는 표현
  - TUI에서 카드가 중첩된 복잡한 레이아웃
  - SSH terminal에서 깨지는 색상 의존 UI

## Product goals

- Goals:
  - Claude Code/Codex 대신 쓸 수 있는 local agent runtime 경험을 제공한다.
  - 저사양 노트북과 Linux server/SSH 환경에서도 session 상태와 monitoring을 확인할 수 있게 한다.
  - 모델별 token, latency, memory, guard, tool, stop-gate metric을 빠르게 스캔하게 한다.
  - 사용자가 raw log를 뒤지지 않고 현재 병목과 실패 원인을 판단하게 한다.
- Non-goals:
  - MVP에서 GUI desktop app 제공
  - MVP에서 remote web dashboard를 기본 제공
  - monitoring을 외부 telemetry로 전송
  - raw prompt/source code 원문을 monitoring DB에 기본 저장
- Success signals:
  - 사용자가 SSH terminal에서 현재 모델, token 사용량, latency, 실패 gate를 5초 안에 파악한다.
  - 긴 agent run 중 pending approval, active tool, subagent/team status, model metric이 한 화면에서 길을 잃지 않게 보인다.
  - benchmark 결과와 실제 run metric을 같은 vocabulary로 이해할 수 있다.

## Personas and jobs

- Primary personas:
  - 한국어 사용자
  - Claude Code/Codex 비용이 부담스러운 개발자
  - 16 GB RAM 수준의 노트북 사용자
  - Linux server나 SSH session에서 local model runtime을 돌리는 사용자
  - 로컬 LLM tooling에 익숙하지 않지만 coding agent 도움을 원하는 사용자
- User jobs:
  - agent session 진행 상태 확인
  - 모델별 token/latency/resource 사용량 비교
  - 현재 backend/model health 확인
  - 실패한 tool/guard/stop gate 원인 파악
  - 승인 대기 action을 안전하게 처리
  - benchmark와 실제 사용 결과 비교
- Key contexts of use:
  - 좁은 terminal pane
  - SSH 접속 Linux server
  - macOS/Windows local terminal
  - long-running coding agent session
  - 모델 benchmark 또는 artifact audit 중

## Information architecture

- Primary navigation:
  - TUI top-level tabs: Session, Monitor, Agents, Evidence, Logs, Settings
  - Keyboard-first navigation with single-key tab switching
  - Command palette for less frequent actions
- Core routes/screens:
  - CLI: `rpotato monitor status`, `rpotato monitor models`, `rpotato monitor session <id>`
  - TUI: monitor overview, model detail, session detail, failures, export/prune
  - Later optional: local HTML report generated from SQLite/export data
- Content hierarchy:
  1. Current run health: model, backend, active workflow, approval state
  2. Token and latency summary
  3. Failure/gate status
  4. Subagent/team breakdown
  5. Detail tables and logs

## Design principles

- SSH-first: every critical monitoring function must work in a plain terminal.
- Dense but calm: show operational data without dashboard decoration.
- Progressive disclosure: overview first, drill down on model/session/tool only when requested.
- Evidence over confidence theater: completion and health claims cite metric/evidence state.
- Policy visibility: approval, privacy, redaction, and stop-gate status must be visible.
- Tradeoffs:
  - TUI cannot compete with HTML for charts, so use compact tables, sparklines, sorted lists, and drill-down panels.
  - HTML can be better for offline reports, but should not become the only monitoring surface.

## Visual language

- Color:
  - Use restrained terminal colors with semantic meaning only.
  - Green: passing/healthy, yellow: degraded/waiting, red: blocked/failed, blue/cyan: selected/focus.
  - Do not rely on color alone; include status text or symbols.
- Typography:
  - Terminal-native monospace.
  - Short Korean labels by default.
  - Avoid long English headings in user-facing TUI.
- Spacing/layout rhythm:
  - Dense rows, stable columns, no layout shift when values update.
  - Fixed status bar and command bar.
- Shape/radius/elevation:
  - Terminal borders are functional separators, not decorative cards.
  - Avoid nested boxes when spacing and headings are enough.
- Motion:
  - Minimal. Use periodic refresh and small progress indicators.
  - No animated noise in monitoring screens.
- Imagery/iconography:
  - No bitmap imagery in TUI.
  - ASCII/Unicode symbols may be used only when they improve scan speed and have text fallback.

## Components

- Existing components to reuse:
  - CLI command output style from current scaffold
  - Runtime status vocabulary from `docs/glossary.md`
  - Observability metric groups from `docs/observability.md`
- New/changed components:
  - Metric summary strip
  - Model comparison table
  - Session timeline
  - Token budget meter
  - Latency sparkline
  - Gate/failure list
  - Approval queue
  - Log/evidence detail panel
  - Export/prune dialog
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

## Accessibility

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

## Responsive behavior

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

## Interaction states

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

## Content voice

- Tone: 짧고 실무적인 한국어
- Terminology:
  - `model run`
  - `token 사용량`
  - `context 사용량`
  - `backend health`
  - `stop gate`
  - `evidence`
  - `redacted`
- Microcopy rules:
  - Failure reasons should say what is blocked and why.
  - Privacy-sensitive panels should explicitly mark redacted data.
  - Do not use marketing copy inside monitoring screens.

## Implementation constraints

- Framework/styling system:
  - Rust TUI framework is not selected yet.
  - TUI must consume runtime state through runtime core contracts, not direct DB ownership.
- Design-token constraints:
  - Semantic color names only: healthy, warning, failed, selected, muted.
  - Fixed width columns need truncation rules.
- Performance constraints:
  - TUI must stay responsive while monitoring long-running sessions.
  - SQLite reads should be bounded and paginated.
  - Live updates should not block approvals.
- Compatibility constraints:
  - SSH/Linux server use is a first-class context.
  - No browser requirement for core monitoring.
  - Optional HTML should be generated or served locally from exported data, not required for baseline operation.
- Test/screenshot expectations:
  - TUI smoke tests at 80x24 and wide terminal sizes.
  - Canvas/browser screenshot tests are not relevant unless the later HTML report/dashboard is implemented.

## Monitoring TUI Screen Contract

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

## HTML Surface Position

HTML can be useful later for richer charts, sharing benchmark reports, or reviewing historical runs.

Initial stance:

- TUI is the primary monitoring surface for local/SSH/server contexts.
- CLI monitor commands are the plain text fallback.
- HTML is a later optional report/dashboard surface.
- HTML must consume the same SQLite/export data and must not create a separate monitoring truth source.

## Open questions

- [ ] Which Rust TUI framework should own the terminal layout?
- [ ] Which SQLite crate should be used?
- [ ] Should optional HTML be a static report export or a local-only web dashboard?
- [ ] What is the default monitoring retention period?
- [ ] What terminal width should be the hard minimum for interactive TUI?
