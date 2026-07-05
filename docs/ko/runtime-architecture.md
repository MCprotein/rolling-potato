# 런타임 Architecture

`rolling-potato`의 제품 본체는 CLI가 아니라 coding agent runtime입니다. CLI는 MVP의 첫 user surface입니다.

목표는 Claude Code/Codex 대신 사용할 수 있는 로컬 agent runtime입니다. Hooks, skills, subagents, team runtime, TUI는 제품 완성에 필요한 1급 runtime capability입니다. Claude Code/Codex형 plugin은 직접 실행하지 않고 adapter를 통해 runtime capability로 변환합니다.

## 계층

```text
User
  -> Surface
     -> Runtime core
        -> Backend adapter
           -> Local inference backend
              -> Model artifact
```

## 사용자 Surface

Surface는 사용자가 runtime에 접근하는 입구입니다.

MVP surface:

- `rpotato` CLI
- `rpotato` TUI after the first CLI vertical slice

나중에 붙을 수 있는 surface:

- IDE extension
- local HTTP control API
- benchmark harness

Surface가 소유하는 것:

- command parsing
- user input capture
- approval prompt rendering
- progress display
- diff/result display
- final report display
- subagent/team status display
- evidence and stop gate display

Surface가 소유하지 않는 것:

- tool permission decision
- model/backend artifact trust decision
- context selection policy
- ontology merge
- patch application
- stop/completion decision

## 런타임 Core

Runtime core는 Claude Code, Codex, 가재코드 같은 agent 경험에서 실제로 중요한 부분을 소유합니다.

Runtime core가 소유하는 것:

- session lifecycle
- session history query와 resume selection
- runtime state
- append-only ledger
- observability projection
- hook lifecycle
- skill registry and invocation
- plugin import, validation, enablement
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

Rule: 모델 출력은 tool 실행 권한이 아닙니다. Runtime core가 모델 출력을 해석하고, 정책 gate를 통과한 action만 실행합니다.

## 백엔드 Adapter

Backend adapter는 runtime core와 추론 backend 사이의 경계입니다.

MVP adapter:

- `llama.cpp` sidecar

Adapter가 제공해야 하는 기능:

- health check
- model metadata
- context length reporting
- chat completion
- streaming tokens
- cancellation
- backend diagnostics

Adapter는 project file, user approval, patch, command policy를 모릅니다. 그 경계는 runtime core에 남깁니다.

## 플러그인 Adapter

Plugin adapter는 외부 runtime의 plugin package를 `rpotato` capability로 변환하는 compatibility boundary입니다.

Adapter가 제공해야 하는 기능:

- source runtime detection
- source manifest parse
- local path canonicalization and remote source rejection
- normalized `rpotato` plugin manifest generation
- skill/hook/subagent/MCP capability mapping
- unsupported capability report
- permission report
- enable/disable state

Adapter는 외부 plugin을 직접 실행하지 않습니다. Codex source runtime adapter를 먼저 구현하고, Claude Code source runtime adapter는 그 뒤에 구현합니다. 외부 marketplace, registry, catalog, mirror, remote URL source는 받지 않습니다. 실행은 변환된 capability가 runtime core의 hook, tool policy, ledger, evidence gate를 통과할 때만 가능합니다.

상세 설계는 [plugin-adapters.md](plugin-adapters.md)를 따릅니다.

## 세션 히스토리와 Resume

세션 재개는 CLI shortcut이 아니라 runtime core가 책임지는 기능입니다.

Runtime은 세 계층을 분리합니다.

- append-only ledger: session event의 audit source
- SQLite projection: CLI/TUI 선택을 위한 query 가능한 session history
- current-state: 현재 선택된 `session_id`와 resume metadata

`rpotato session list`, `rpotato session history`, id 없는 `rpotato resume`은 SQLite에서 선택 가능한 history를 읽습니다. `rpotato session resume <session-id>`와 `rpotato resume <session-id>`는 선택한 session id를 current-state에 기록해 이후 명령이 선택된 session identity 아래에 이어 기록되게 합니다. 후속 agent-loop phase는 이 선택된 session으로 transcript를 replay하고 context를 재구성한 뒤 대화를 이어갑니다.

## 모델 Artifact

Model artifact는 runtime의 소유물이 아닙니다. Third-party artifact로 원 출처와 license를 유지합니다.

Runtime core는 다음이 확인되기 전까지 model install을 차단합니다.

- upstream source
- artifact provider
- artifact URL
- license
- file size
- SHA-256
- backend compatibility

## 제어 Flow

`rpotato run "테스트 실패 고쳐줘"`의 기본 흐름:

1. CLI surface가 user request를 runtime core에 전달한다.
2. Runtime core가 matching skill과 mode를 해석한다.
3. Runtime core가 project boundary와 state를 연다.
4. Runtime core가 hook pipeline을 초기화한다.
5. Runtime core가 Layer A repo facts와 Layer B ontology를 조회한다.
6. Runtime core가 필요한 source pointer를 원본 파일 read로 승격한다.
7. Runtime core가 필요할 때 bounded subagent나 team stage를 생성한다.
8. Runtime core가 model/backend adapter에 제한된 prompt/action request를 보낸다.
9. Runtime core가 모델 output을 action candidate로 해석한다.
10. Runtime core가 permission policy와 evidence gate를 적용한다.
11. CLI/TUI surface가 필요한 승인 prompt 또는 diff를 표시한다.
12. Runtime core가 승인된 action만 실행한다.
13. Runtime core가 검증 결과와 evidence를 ledger에 기록한다.
14. Runtime core가 token, latency, backend, guard, tool metric을 local SQLite projection에 반영한다.
15. Stop gate가 완료 여부를 판정한다.
16. Reporter 출력은 Korean output guard를 통과한 뒤 surface가 표시한다.

## 양보할 수 없는 경계

- CLI surface는 runtime policy를 우회하지 않는다.
- TUI surface는 runtime policy를 우회하지 않는다.
- Backend adapter는 file write나 command execution을 직접 수행하지 않는다.
- Plugin adapter는 foreign plugin code를 직접 실행하지 않는다.
- Model output은 곧바로 shell command나 patch가 되지 않는다.
- Hooks can narrow behavior but cannot widen permissions.
- Skills declare requirements but do not execute tools directly.
- Subagents and teams inherit parent runtime policy.
- Snippet은 authoritative source가 아니다.
- Ontology claim은 source ref와 confidence 없이 확정하지 않는다.
- Stop은 모델이 아니라 evidence gate가 결정한다.
- Monitoring은 외부 telemetry가 아니라 local-first runtime state다.
