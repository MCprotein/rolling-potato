# 저장소 전면 리팩터링 계획

상태: 실행 중 (2026-07-29 기준)  
대상 branch: `fix/autonomous-web-grounding`  
원칙: 동작 보존 회귀 테스트 → 책임 분리 → targeted 검증 → 논리 커밋·푸시

## 목표

`rolling-potato`를 작은 로컬 모델에서도 예측 가능하게 동작하는 범용 코딩
에이전트로 유지하면서 다음 문제를 제거한다.

- 사용자 사례가 추가될 때마다 자연어 문구와 제품명을 조건문에 추가하는 구조
- 오케스트레이션, 정책 판단, 외부 I/O, 표시가 한 파일에서 함께 바뀌는 구조
- 수백 개의 문자열 검사를 한 테스트 파일에 누적한 취약한 아키텍처 검증
- 동일 내용을 영어·한국어 장문 문서에 수동으로 중복 기록하는 방식
- 1,000줄 안팎 파일을 읽어야만 한 책임의 전체 흐름을 알 수 있는 구조

줄 수 감소 자체가 완료 조건은 아니다. 변경 이유가 하나이고, 외부 의존 방향이
명확하며, 테스트가 공개 동작을 검증하는지가 완료 기준이다.

## 보존할 사용자 동작

- `rpotato`만 실행하면 대화 중심 TUI가 열린다.
- 일반 질문, 코딩 요청, 웹 근거가 필요한 질문을 같은 입력창에서 처리한다.
- 모델이 도구 사용을 판단하되 runtime이 허용 범위, 예산, 비밀값 유출 및 결과
  근거를 검증한다.
- 작은 모델이 도구 결정을 잘못하거나 불완전한 응답을 반환해도 내부 프로토콜을
  사용자 답변으로 노출하지 않는다.
- 대화, 명시적 `/resume`, context 계산, compaction, 이미지 첨부 및 projector
  on-demand 상태를 서로 다른 계약으로 유지한다.
- 웹 검색은 별도 API key나 검색 SaaS 없이 `Search`, `Open`, `Find`를 제공한다.
- 기존 설치·업데이트·모델 cache·backend 상태와 직전 schema를 계속 읽을 수 있다.

## 시작 기준선

2026-07-29 리팩터링 시작 전 실측:

| 범위 | 가장 큰 파일 | 줄 수 | 문제 |
| --- | --- | ---: | --- |
| 아키텍처 테스트 | `tests/architecture_contract.rs` | 6,989 | 모든 도메인의 문자열 구조 검사가 한 파일에 집중 |
| PTY 지원 | `tests/support/platform/native_terminal.rs` | 1,775 | process, input, capture, fixture 책임 혼재 |
| patch 통합 테스트 | `tests/patch/lifecycle.rs` | 1,737 | 서로 다른 lifecycle 사례가 한 모듈에 집중 |
| CLI parser 테스트 | `src/surfaces/cli/parser/tests/mod.rs` | 1,427 | 명령군별 fixture 소유권 불명확 |
| TUI 대화 | `src/app/tui_adapter/conversation.rs` | 1,035 | local facts, prompt, tool decision, sanitization 혼재 |
| system install | `src/adapters/system_install.rs` | 1,000 | plan, profile, binary, cleanup I/O 혼재 |
| llama backend | `src/adapters/llama_cpp/backend.rs` | 937 | protocol, process, request, response 책임 혼재 |
| 릴리스 문서 | `RELEASE_NOTES.md` | 3,632 | 모든 과거 릴리스를 단일 문서에 누적 |
| 한국어 릴리스 문서 | `docs/ko/RELEASE_NOTES.md` | 3,429 | 영어 문서와 수동 중복 |
| 실행 회고 | `docs/agent-retrospectives.md` | 1,328 | runtime, CI, release 사고가 단일 문서에 누적 |

시작 시점 branch의 웹 단위 테스트 68개는 통과했지만
`web_search_open_find_have_separate_bounded_owners`는
`answer_binding.rs`가 300줄을 넘어서 실패한다. 이 branch는 기능 회귀뿐 아니라
구조 계약도 복구하기 전까지 candidate로 전환하지 않는다.

## 진행 현황

아래 상태는 2026-07-29 현재 작업 tree의 파일 경계와 완료된 targeted 검증을
기준으로 한다. `구조 변경 완료`는 해당 단계의 책임 분리와 targeted 검증이
끝났다는 뜻이며, PR candidate 전체 검증이나 merge 완료를 뜻하지 않는다.

| 단계 | 상태 | 확인된 결과 | 남은 종료 조건 |
| --- | --- | --- | --- |
| R1 하드코딩 제거 | 1차 경계 완료 | domain-specific grounding 분기를 generic query plan과 typed answer binding으로 교체하고 web·conversation owner를 분리 | 최종 candidate HEAD의 PR 검증 |
| R2 아키텍처 테스트 | 1차 경계 완료 | `tests/architecture_contract.rs`를 400줄 미만 facade와 domain별 contract owner로 분리 | 최종 candidate HEAD의 전체 architecture contract |
| R3 TUI | 하드닝 중 | attachment, keymap, render, native PTY process·capture·fixture 책임을 별도 module로 분리 | 잔여 setup·conversation·action owner 점검과 최종 native-terminal candidate 검증 |
| R4 session/context | 하드닝 중 | context usage 정본 계산, compaction domain, session memory·restoration·assembly 경계를 분리 | 기본 대화·누적 context·compaction·`/resume` candidate journey |
| R5 backend/model/install | 하드닝 중 | llama protocol·process, model artifact·download, install plan·mutation 책임을 분리 | 잔여 production hotspot 점검과 managed backend·model cache hit candidate 검증 |
| R6 workflow/patch/collaboration | 하드닝 중 | recovery, transaction, snapshot, patch use case, collaboration persistence·report 경계를 분리하고 관련 targeted 검증을 통과 | 잔여 transition·transcript·terminal·collaboration owner 점검과 최종 PR 검증 |
| R7 문서 | 1차 경계 완료 | 영문·한국어 release notes와 실행 회고를 짧은 index와 800줄 미만 archive로 분리하고, architecture·product plan과 release·development runbook 경계를 index에서 구분 | 최종 candidate HEAD의 문서·링크 검증 |

## 구조 하드닝 체크포인트

1차 단계가 끝났다는 이유만으로 전체 리팩터링을 완료 처리하지 않는다.
2026-07-29 추가 계측에서 다음 production 경계를 더 분리했다.

- workflow recovery journal의 계약, projection, transaction, validation
- workflow transaction coordinator의 approval, event sequence, state transition,
  terminal action, verification
- workflow snapshot의 lease, session, TUI read model
- SQLite observability projection의 lifecycle, port, query, store
- TUI adapter 회귀 테스트의 controller, outcome, render, report, view state
- context assembly의 declared files, ontology, durable resume
- patch application facade의 approval dispatch, proposal API, verification evidence,
  shared value helper
- policy decision의 command, project path, schema, value object·port
- collaboration team의 admission, dispatch, ownership, policy, governor, event,
  value object
- TUI conversation 회귀의 decision, local facts, presentation, context assembly
- first-run setup의 orchestration과 model list·confirmation presentation

명시적 hotspot 감사 결과는 다음과 같다. `유지`는 분리를 생략했다는 뜻이 아니라,
현재 파일이 하나의 변경 이유와 불변식을 가지며 더 나누면 오히려 계약이
분산된다는 판단이다.

| 경계 | 결과 | 근거 |
| --- | --- | --- |
| `src/surfaces/tui/setup.rs` | 분리 완료 | setup pipeline·port와 model list·confirmation presentation을 별도 owner로 이동 |
| `src/app/tui_adapter/conversation.rs` | 분리 완료 | production facade를 유지하고 decision, local facts, presentation, context assembly 회귀를 독립 test owner로 이동 |
| `src/composition/tui_action.rs` | 유지 | 의미 기반 action module 등록과 intent dispatch만 소유하는 bounded facade |
| workflow transition·source install·transcript | 분리 완료 | bundle construction, codec, path, validation, projection, storage owner와 짧은 facade 확인 |
| patch terminal·observability analytics | 분리 완료 | terminal mutation use case와 analytics query 종류를 독립 owner로 이동 |
| `src/runtime_core/knowledge/recall.rs` | 유지 | pair 보존, token budget, lexical recall이 하나의 query-driven recall 전략 불변식을 구성 |
| collaboration subagent·team | 분리 완료 | durable record와 team admission·dispatch·ownership·governor 책임을 독립 owner로 이동 |
| `src/runtime_core/policy/decision.rs` | 분리 완료 | exact argv, project path, schema, value object·port를 독립 owner로 이동 |

최종 중단 조건은 잔여 목록의 줄 수가 아니라 다음 네 조건이다.

1. 각 파일이 하나의 변경 이유를 가지거나, 더 나누면 불변식이 분산된다는 근거가 있다.
2. facade는 등록·재수출만 소유하고 use case, policy, I/O 구현이 되돌아오지 못하도록
   architecture contract가 잠긴다.
3. 변경 동작의 targeted 회귀와 정확한 ownership ledger 검증이 통과한다.
4. 최종 candidate HEAD에서 preflight, 독립 리뷰 한 번, PR 전체 CI가 통과한다.

## Machine-readable ownership ledger 예외

`docs/architecture-migration-map.json`은 설명 문서가 아니라 모든 governed path의
정확한 책임 slice, target, proof와 migration lifecycle을 기록하는
machine-readable ledger다. 따라서 prose 문서의 1,000줄 제한을 적용하지 않는다.

| 항목 | 계약 |
| --- | --- |
| Owner | `tests/architecture_contract/migration_map.rs`의 exact coverage contract |
| 예외 이유 | `src`, `tests`, workflow, release script, 문서의 모든 governed file을 누락·중복 없이 기계적으로 대조하려면 전체 inventory가 필요하다. 사람이 읽는 장문 설명과 달리 줄 수 자체가 탐색 비용이나 책임 혼합을 의미하지 않는다. |
| 제거 조건 | Recursive exact coverage와 slice lifecycle 검증을 보존하는 생성·분할 형식으로 교체되고, 새 형식이 동일한 누락·중복·stale record 검사를 통과할 때 예외를 제거한다. 단순 줄 수 감소를 위해 ledger를 임의로 나누지 않는다. |

이 예외는 prose 문서에 확장되지 않는다. Release notes와 실행 회고는 각각
현재 train 또는 주제 index와 bounded archive 구조를 유지한다.

## 참조 구현에서 가져올 원칙

참조는 구조와 상호작용 개념만 사용한다. 코드, 비공개 문구 및 제품별 API를
복사하지 않는다.

### OpenAI Codex

참조: `openai/codex` `d06c7ac055920c7cb140c25ebda3f3db20197b45`

- 웹 기능은 `ext/web-search` extension이 등록하고 tool executor가 schema와 실행을
  소유한다. core는 구체 검색 transport를 직접 알지 않는다.
- tool registry, router, handler, lifecycle을 분리한다.
- context history의 저장·정규화·update를 별도 모듈로 둔다.
- TUI는 app event, history UI, input, bottom pane, render 및 snapshot 검증을
  별도 책임으로 둔다.

적용:

- rpotato의 웹 판단은 domain policy, tool contract, research session, transport,
  presentation으로 나눈다.
- TUI가 web adapter의 세부 검색 규칙이나 backend wire 형식을 알지 않게 한다.

### Claude Code harness

참조: 로컬 `../harness/claude-code`
`e2f01754cb8f237dd61520a3e84c65d7d862f64a`

- system/user context 수집을 query loop와 분리하고 세션 단위로 cache한다.
- 붙여넣기 payload는 입력 표시용 reference와 실제 저장 payload를 구분한다.
- command history, text input, paste 처리, virtual scroll을 독립 책임으로 둔다.
- tool 정의와 query orchestration을 분리한다.

적용:

- attachment와 paste는 composer 문자열이 아니라 typed attachment identity로
  전달한다.
- history navigation과 대화 transcript 복원은 분리한다.
- 입력 keymap, viewport, virtualized history는 controller에서 분리한다.

피할 점:

- `QueryEngine.ts`와 같은 대형 중앙 오케스트레이터를 새로 만들지 않는다.

### 가재코드

참조: `Yeachan-Heo/gajae-code`
`0464f0f92a278ed34059db48d9951cbf41c1ce78`

- agent loop, provider AI, coding-agent 기능, TUI를 패키지 경계로 나눈다.
- stable prefix와 append-only message log를 분리해 context cache와 변경 감지를
  명시한다.
- session manager와 storage port를 분리하고 compaction을 typed session event로
  저장한다.
- keybinding을 의미 기반 action registry로 정의하고 실제 키 조합은 설정으로
  해석한다.

적용:

- rpotato의 context는 immutable instruction prefix, append-only turns,
  compacted summary, current request 예산을 따로 계산한다.
- session event와 filesystem/sqlite 저장 구현을 분리한다.
- TUI controller는 `Submit`, `MoveWord`, `ScrollHistory`, `Choose` 같은 의미 action만
  받는다.

피할 점:

- `agent.ts`, `agent-loop.ts`의 대형 orchestration 파일 구조를 그대로 따르지 않는다.

## 목표 아키텍처

의존 방향:

```text
surfaces (CLI/TUI)
  -> application use cases
    -> runtime_core domain + ports
      <- adapters (filesystem, sqlite, llama.cpp, web, terminal)
```

### 1. 요청과 도구 판단

- `RequestIntent`: local answer, local task, web research, browser interaction
- `ToolCall`: 이름과 검증된 typed input
- `GroundingRequirement`: freshness, public evidence, comparison, explicit browse
- `GroundingPolicy`: generic feature를 평가하고 제품명·행사명은 알지 않는다.
- 모델은 구조화된 결정을 제안하고 runtime policy가 최종 admission을 결정한다.

허용되는 자연어 목록은 언어별 generic signal catalog로 한정한다. 특정
`월드컵`, `Gemma`, `Qwen`, 특정 회사명이나 한 사용자 문장을 runtime 분기에
추가하지 않는다.

### 2. 웹 리서치

- `WebToolContract`: Search/Open/Find schema와 입력 상한
- `WebResearchPolicy`: 단계, 시간, network, evidence budget
- `WebResearchSession`: 상태 전이만 소유
- `WebSearchPort`, `WebPagePort`: 외부 I/O port
- `EvidenceSelector`: lexical overlap, source authority, diversity를 generic하게 평가
- `GroundedAnswerContract`: answer status와 runtime source id를 구조적으로 검증
- `WebPresentation`: citation과 source URL을 runtime-owned 값으로 렌더링

도메인별 답변 문구를 검사하는 `winner`, `license`, `Gemma`, `Qwen` 분기는
제거한다. 답변 품질은 typed status, source binding, evidence coverage 및
완성된 구조로 검증한다.

### 3. 대화와 context

- `ConversationHistory`: 사용자·assistant·tool turn
- `InstructionPrefix`: 모델·도구가 바뀔 때만 invalidation
- `ContextBudget`: 모델 manifest의 실제 context와 reserved output을 계산
- `ContextAssembler`: history, summary, attachment, current request 조립
- `CompactionPolicy`: threshold와 보존 범위 결정
- `CompactionStore`: summary artifact 저장

TUI 표시용 context 수치는 assembler가 실제 backend에 전달할 입력을 기준으로
계산한다. 표시를 위한 별도 추정 경로를 두지 않는다.

### 4. TUI

- input action/keymap
- composer state
- attachment clipboard
- command palette/picker
- transcript viewport/scroll
- request progress
- status line
- markdown presentation

각 구성 요소는 view model을 받고 직접 filesystem, backend 또는 web adapter를
호출하지 않는다.

### 5. backend와 모델

- backend lifecycle state machine
- llama.cpp process supervisor
- HTTP request/response codec
- structured-output capability
- model artifact repository
- projector artifact repository
- selection/cache use case

언어 모델 weight와 projector cache 상태는 서로 다른 value object로 유지한다.

### 6. workflow, patch, collaboration

- domain state와 transition
- application transaction
- persistence port
- adapter codec
- report projection

같은 enum을 여러 layer에서 문자열로 재해석하지 않는다. compatibility codec은
storage 경계에만 둔다.

## 실행 순서

### R1. 현재 draft PR의 하드코딩 제거

1. 기존 웹·대화 회귀 테스트를 보존한다.
2. domain-specific query enrichment를 generic query plan으로 교체한다.
3. answer binding의 질문 유형별 문자열 검사를 구조 검증으로 교체한다.
4. `conversation.rs`에서 local facts, decision prompt, tool routing을 분리한다.
5. web architecture contract와 사용자 사례를 통과시킨다.

### R2. 아키텍처 테스트 분리

1. `architecture_contract.rs`를 domain별 test target으로 분리한다.
2. 단순 `source.contains(...)` 검사를 compile visibility, module ownership,
   fixture behavior 검사로 가능한 만큼 교체한다.
3. 예외는 owner, 이유, 제거 조건을 가진 명시적 목록으로 제한한다.

### R3. TUI 분리

1. conversation/controller 테스트를 사용자 journey별로 분리한다.
2. 입력, attachment, viewport, picker, progress, markdown을 독립 component로
   이동한다.
3. native PTY는 process driver, semantic readiness, keystroke, capture로 분리한다.

### R4. session/context 분리

1. append-only history와 resume projection을 분리한다.
2. context assembly와 status estimate를 하나의 정본 계산으로 통합한다.
3. compaction event, summary artifact, recent tail을 독립 계약으로 검증한다.

### R5. backend/model/install 분리

1. llama protocol과 process lifecycle을 분리한다.
2. model/projector artifact repository와 cache state를 분리한다.
3. install plan과 filesystem mutation을 분리한다.

### R6. workflow/patch/collaboration 분리

도메인 state, use case, persistence adapter, report projection 순서로 한 cluster씩
이동한다. durable bytes와 event ordering은 golden fixture로 먼저 고정한다.

### R7. 문서 구조 정리

1. `RELEASE_NOTES.md`는 최근 버전 index로 축소하고 과거 기록은 release train별
   문서로 분리한다.
2. 한국어 release notes도 같은 버전 파일 경계를 사용한다.
3. `agent-retrospectives.md`는 index로 바꾸고 runtime, CI, release, governance
   회고를 주제별 문서로 이동한다.
4. `PLAN.md`, architecture, TUI, capability 문서는 개념 설명과 운영 runbook을
   분리한다.
5. 문서 index와 내부 링크 검증을 추가한다.

## 품질 규칙

- 새 dependency를 추가하지 않는다.
- production 파일 600줄 초과는 새로 만들지 않는다.
- 기존 600줄 초과 파일은 책임 이동 전 임시 예외로 기록하고 순차적으로 제거한다.
- test fixture도 800줄 초과 단일 파일을 새로 만들지 않는다.
- 사용자 문장을 그대로 조건문에 추가해 회귀를 막지 않는다.
- adapter 실패를 `ok()`, 빈 값 또는 임의 기본값으로 숨기지 않는다.
- compatibility fallback은 외부 버전 경계, 보존 이유, primary/fallback 테스트를
  모두 가진 경우만 유지한다.
- Rust에서는 상속 중심 OOP 대신 작은 value object, 명시적 state machine,
  port/adapter, strategy 및 repository 패턴을 사용한다.
- trait는 실제 대체 구현이나 I/O 경계가 있을 때만 만들고 단일 함수 wrapper를
  늘리지 않는다.

## 검증과 커밋

각 단위:

1. 보존 동작 targeted test
2. 한 책임의 코드 이동 또는 단순화
3. 같은 targeted test와 관련 architecture contract
4. `cargo fmt --check`
5. Conventional Commit
6. 현재 branch push

전체 `cargo test`, clippy, build 및 release gate는 draft PR의 최종 candidate
HEAD에서 CI가 한 번 수행한다. 같은 HEAD에서 로컬 전체 검증을 반복하지 않는다.

## 완료 조건

- domain-specific grounding 예외가 production 코드에서 제거된다.
- 모든 1,000줄 이상 production/test 파일이 책임별 module로 분리된다.
- 600줄 초과 production 파일은 근거가 있는 예외만 남고 새 증가를 CI가 막는다.
- 아키텍처 검증이 한 7,000줄 문자열 검사 파일에 집중되지 않는다.
- 문서의 1,000줄 이상 단일 파일이 index와 주제 문서로 분리된다.
- 기본 대화, 자동 웹 검색, Search/Open/Find, 이미지 첨부, context 누적,
  compaction, scroll, `/resume`, model cache hit가 사용자 journey로 검증된다.
- draft PR이 candidate preflight를 통과하기 전에는 ready, merge 또는 release하지
  않는다.
