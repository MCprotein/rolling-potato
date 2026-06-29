# TUI Surface

TUI는 Claude Code/Codex replacement experience에 필요한 필수 product surface입니다.

첫 구현은 CLI command에서 시작할 수 있지만, target runtime은 interactive work를 위한 terminal UI를 지원해야 합니다.

## 목표

- long-running agent session을 inspect 가능하게 만든다.
- log를 직접 뒤지지 않아도 runtime state를 보여준다.
- approval, diff, tool output, subagents, teams를 지원한다.
- context/evidence/stop gate를 visible하게 만든다.
- keyboard-first terminal workflow를 유지한다.

## 비목표

- GUI desktop app
- primary interface로서의 web dashboard
- TUI-owned policy
- runtime core 직접 우회

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
- evidence/stop gate status
- logs and diagnostics

## Interaction Model

TUI action:

- user request submit
- tool call approve 또는 deny
- patch approve 또는 deny
- command approve 또는 deny
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

## Runtime Contract

TUI는 runtime state를 consume합니다.

- session status
- active workflow
- active skill
- active subagents
- active team stage
- pending approvals
- ledger tail
- evidence status
- backend/model status
- token/resource metric summary

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
- Korean output guard가 final report에 visible
