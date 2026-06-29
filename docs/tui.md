# TUI Surface

TUI는 Claude Code/Codex replacement experience에 필요한 필수 product surface입니다.

첫 구현은 CLI command에서 시작할 수 있지만, target runtime은 interactive work를 위한 terminal UI를 지원해야 합니다.

TUI design source of truth는 [DESIGN.md](../DESIGN.md)입니다. 특히 monitoring 화면은 SSH/Linux server에서도 쓸 수 있어야 하므로 browser나 GUI를 전제하지 않습니다.

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

## Required Views

최소 TUI view:

- chat/session transcript
- current plan
- context and ontology summary
- pending approvals
- diff viewer
- tool output viewer
- model/backend status
- model/token usage summary
- subagent status
- team status
- plugin permission review
- evidence/stop gate status
- logs and diagnostics

## Interaction Model

TUI action:

- user request submit
- tool call approve 또는 deny
- patch approve 또는 deny
- command approve 또는 deny
- plugin enable 또는 disable
- source pointer inspect
- evidence inspect
- active view switch
- workflow cancel
- workflow resume

모든 action은 runtime core를 통과합니다.

## Layout Direction

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

## Runtime Contract

TUI는 runtime state를 consume합니다.

- session status
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
- cancel
- resume
- inspect

## Accessibility And Constraints

- Korean user-facing label by default
- small terminal size에서도 readable
- SSH/Linux server 환경 first-class
- hidden destructive shortcut 금지
- keyboard-first
- terminal resize handling
- clear fail-closed error display

## Validation

TUI는 smoke test가 필요합니다.

- small terminal size layout render
- approval flow가 runtime policy를 bypass하지 않음
- diff view가 long file을 처리함
- cancellation이 runtime state를 update함
- team/subagent status update
- model/token monitoring view updates
- plugin permission review가 runtime policy를 bypass하지 않음
- Korean output guard가 final report에 visible
