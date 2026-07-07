# 터미널 UI Surface

TUI는 Claude Code/Codex replacement experience에 필요한 필수 product surface입니다.

첫 구현은 CLI command에서 시작할 수 있지만, target runtime은 interactive work를 위한 terminal UI를 지원해야 합니다.

TUI design source of truth는 [DESIGN.md](../DESIGN.md)입니다. 특히 monitoring 화면은 SSH/Linux server에서도 쓸 수 있어야 하므로 browser나 GUI를 전제하지 않습니다.

## 현재 Beta Surface

`v0.5.0`은 dependency-free, read-only ASCII TUI beta를 추가합니다.

- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

이 beta는 기존 runtime state와 SQLite observability projection을 읽습니다. Project/session 상태, model/token summary, session history, resume hint, read-only boundary를 보여줍니다. Approval, patch apply, resume, cancel, workflow mutation은 수행하지 않습니다.

`v0.6.0`은 같은 read-only beta에 patch proposal inspection을 추가합니다.

- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`

Approval view는 project-local `.rpotato/patch-proposals/` record를 읽고 proposal status, id, path, replacement count를 보여줍니다. Diff view는 proposal metadata, approval/dry-run command hint, 저장된 unified diff를 보여주며 patch approve나 apply는 수행하지 않습니다.

`v0.7.0`은 read-only beta에 선택한 session의 event inspection을 추가합니다.

- `rpotato tui transcript <session-id>`

Transcript view는 현재 project의 SQLite ledger projection을 읽고 session metadata와 timestamp 순 event timeline을 보여줍니다. Raw model transcript replay, conversation continuation, raw event detail 기본 노출은 의도적으로 수행하지 않습니다.

`v0.8.0`은 read-only beta에 evidence/stop-gate status inspection을 추가합니다.

- `rpotato tui evidence`

Evidence view는 runtime evidence JSONL path, project evidence directory, SQLite `evidence_records` count, SQLite `stop_gate_results` count, stale policy summary를 읽습니다. Workflow를 pass/fail 판정하지는 않으며, terminal stop-gate evaluation은 runtime-core 후속 작업입니다.

첫 beta의 framework decision은 dependency-free terminal-safe surface로 시작하는 것입니다. Interaction requirement가 안정된 뒤 더 풍부한 TUI crate가 필요한지 결정합니다.

## 목표

- long-running agent session을 inspect 가능하게 만든다.
- log를 직접 뒤지지 않아도 runtime state를 보여준다.
- approval, diff, tool output, subagents, teams를 지원한다.
- plugin import/permission review를 지원한다.
- context/evidence/stop gate를 visible하게 만든다.
- model/token/resource monitoring을 terminal-only 환경에서도 사용할 수 있게 만든다.
- keyboard-first terminal workflow를 유지한다.

## 비목표

- GUI desktop app
- primary interface로서의 web dashboard
- TUI-owned policy
- runtime core 직접 우회
- monitoring 화면에서 raw prompt/source를 기본 노출하는 것

## 필수 View

최소 TUI view:

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
- team status
- plugin permission review
- evidence/stop gate status
- logs and diagnostics

## 상호작용 Model

TUI action:

- user request submit
- tool call approve 또는 deny
- patch approve 또는 deny
- command approve 또는 deny
- plugin enable 또는 disable
- blocked plugin capability를 per-capability로 approve 또는 deny
- source pointer inspect
- evidence inspect
- active view switch
- session history 열기
- 선택한 session resume
- workflow cancel
- workflow resume

모든 action은 runtime core를 통과합니다.

## Layout 방향

초기 layout:

```text
┌────────────────────────────────────────────┐
│ transcript / active task                   │
├───────────────┬────────────────────────────┤
│ plan/context  │ diff/tool/evidence detail  │
├───────────────┴────────────────────────────┤
│ approvals / status / command bar           │
└────────────────────────────────────────────┘
```

Monitoring layout direction:

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
- active/degraded/blocked run first in sort order
- every metric shows timestamp or stale marker
- no color-only status; include text status
- no raw prompt/source by default
- export and prune actions require dry-run summary
- narrow terminal falls back to stacked single-panel views

## Runtime 계약

TUI는 runtime state를 consume합니다.

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

TUI는 user decision을 emit합니다.

- request
- approve
- deny
- session 선택
- cancel
- resume
- inspect

## 명령 Palette Routing

Phase 3에서 고정한 command palette routing contract:

- `request.submit` -> `rpotato run <request>`
- `intent.preview` -> `rpotato intent classify <request>`
- `skill.run` -> `rpotato skill run <id>`
- `plugin.review` -> `rpotato plugin inspect <id>` 또는 `rpotato plugin validate <id>`
- `plugin.toggle` -> `rpotato plugin enable <id>` 또는 `rpotato plugin disable <id>`
- `workflow.cancel` -> `rpotato cancel`
- `session.history` -> `rpotato session list`
- `session.resume` -> `rpotato resume <session-id>`
- `workflow.resume` -> `rpotato state resume`
- `monitor.open` -> `rpotato monitor status`
- `evidence.inspect` -> `rpotato evidence validate <artifact-pointer>`

Active workflow는 current-state가 소유합니다. TUI action은 runtime core에 request만 emit하고, skill/plugin/subagent/team은 parent workflow pointer 없이 독립 workflow를 만들 수 없습니다.

## 접근성과 제약

- Korean user-facing label by default
- small terminal size에서도 readable
- SSH/Linux server 환경 first-class
- hidden destructive shortcut 금지
- keyboard-first
- terminal resize handling
- clear fail-closed error display

## 검증

TUI는 smoke test가 필요합니다.

- small terminal size layout render
- approval flow가 runtime policy를 bypass하지 않음
- diff view가 long file을 처리함
- cancellation이 runtime state를 update함
- team/subagent status update
- model/token monitoring view updates
- plugin permission review가 runtime policy를 bypass하지 않음
- shell/bin/MCP/background/remote/file-write capability가 기본 차단으로 표시됨
- Korean output guard가 final report에 visible
