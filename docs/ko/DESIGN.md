# 디자인

## 1. 제품 기반

### 기준 문서

- 상태: 활성
- 마지막 갱신: 2026-07-22
- 주요 제품 surface: 인자 없는 `rpotato` TUI; 자동화·진단용 subcommand CLI; 선택 가능한 로컬 정적 HTML report
- 검토한 근거:
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
  - 2026-07-22 사용자가 제공한 Codex·Claude Code 터미널 참조 화면

### 브랜드

- 성격: 작고 빠르며 실용적인 로컬 코딩 에이전트 runtime
- 신뢰 신호: local-first 실행, 명시적 승인, 출처 기반 모델 claim, 보이는 evidence, 한국어 final report
- 피할 것:
  - SaaS dashboard처럼 과하게 장식적인 화면
  - 모델을 마법처럼 보이게 하는 표현
  - TUI에서 카드가 중첩된 복잡한 레이아웃
  - SSH terminal에서 깨지는 색상 의존 UI

### 제품 목표

- 목표:
  - `rpotato` 실행만으로 첫 설정과 대화가 이어지는 코딩 에이전트 TUI에 진입한다.
  - Claude Code/Codex 대신 쓸 수 있는 local agent runtime 경험을 제공한다.
  - 저사양 노트북과 Linux server/SSH 환경에서도 session 상태와 monitoring을 확인할 수 있게 한다.
  - 모델별 token, latency, memory, guard, tool, stop-gate metric을 빠르게 스캔하게 한다.
  - 사용자가 raw log를 뒤지지 않고 현재 병목과 실패 원인을 판단하게 한다.
- 비목표:
  - MVP에서 GUI desktop app 제공
  - MVP에서 remote web dashboard를 기본 제공
  - monitoring을 외부 telemetry로 전송
  - raw prompt/source code 원문을 monitoring DB에 기본 저장
- 성공 신호:
  - 사용자가 SSH terminal에서 현재 모델, token 사용량, latency, 실패 gate를 5초 안에 파악한다.
  - 긴 agent run 중 pending approval, active tool, subagent/team status, model metric이 한 화면에서 길을 잃지 않게 보인다.
  - benchmark 결과와 실제 run metric을 같은 용어로 이해할 수 있다.

### Persona와 사용 목적

- 주요 persona:
  - 한국어 사용자
  - Claude Code/Codex 비용이 부담스러운 개발자
  - 16 GB RAM 수준의 노트북 사용자
  - Linux server나 SSH session에서 local model runtime을 돌리는 사용자
  - 로컬 LLM tooling에 익숙하지 않지만 coding agent 도움을 원하는 사용자
- 사용자가 하려는 일:
  - agent session 진행 상태 확인
  - 모델별 token/latency/resource 사용량 비교
  - 현재 backend/model health 확인
  - 실패한 tool/guard/stop gate 원인 파악
  - 승인 대기 action을 안전하게 처리
  - benchmark와 실제 사용 결과 비교
- 주요 사용 환경:
  - 좁은 terminal pane
  - SSH 접속 Linux server
  - macOS/Windows local terminal
  - 오래 실행되는 coding agent session
  - 모델 benchmark 또는 artifact audit

## 2. 사용 경험 체계

### 정보 구조

- 주 navigation:
  - 기본 surface는 운영 dashboard가 아니라 하나의 대화 transcript다.
  - composer는 화면 하단에 유지되며 자연어 코딩 요청을 받는다.
  - model, status, session, doctor 등 보조 화면은 사용자가 slash command로 요청할 때만 연다.
- 핵심 route/screen:
  - TUI 기본: 짧은 welcome, transcript, composer, runtime status line 하나
  - TUI 보조: model picker, status summary, session history, diagnostics, approvals, evidence
  - CLI: `rpotato monitor status`, `rpotato monitor models`, `rpotato monitor session <id>`
  - 선택 가능한 local report: `rpotato monitor export --format html`
- 내용 우선순위:
  1. 현재 user·assistant turn
  2. active task 진행 상태, 승인 또는 조치 가능한 실패
  3. composer
  4. 간결한 model/context/backend/session 상태
  5. 상세 metric, hash, ledger 상태, log는 명시적인 진단 view에서만 표시

### 디자인 원칙

- SSH-first: 모든 중요한 monitoring 기능은 plain terminal에서 동작해야 한다.
- Conversation first: 최초 화면은 ledger viewer가 아니라 코딩 도우미처럼 보여야 한다.
- Calm by default: 사용자가 진단을 요청하기 전에는 hash, revision, ledger count, projection 상태, raw workflow field를 보여주지 않는다.
- Progressive disclosure: transcript를 먼저 보여주고, 요청 시 model/session/tool/monitoring detail로 drill down한다.
- Evidence over confidence theater: 완료와 health claim은 metric/evidence state를 근거로 한다.
- Policy visibility: approval, privacy, redaction, stop-gate 상태가 보여야 한다.
- tradeoff:
  - TUI는 HTML chart와 경쟁할 수 없으므로 compact table, sparkline, sorted list, drill-down panel을 사용한다.
  - HTML은 offline report에 더 적합할 수 있지만 유일한 monitoring surface가 되면 안 된다.

### 시각 언어

- 색상:
  - 의미가 있는 경우에만 절제된 terminal color를 사용한다.
  - green: passing/healthy, yellow: degraded/waiting, red: blocked/failed, blue/cyan: selected/focus를 뜻한다.
  - 색상에만 의존하지 말고 status text나 symbol을 함께 둔다.
- typography:
  - terminal-native monospace를 사용한다.
  - user-facing TUI는 짧은 한국어 label을 기본으로 한다.
  - 긴 영어 heading은 피한다.
- spacing/layout rhythm:
  - dense row, stable column, 값 갱신 시 layout shift 없음.
  - 고정 status bar와 command bar.
- shape/radius/elevation:
  - terminal border는 장식 카드가 아니라 기능적 구분선이다.
  - spacing과 heading으로 충분하면 nested box를 피한다.
- motion:
  - 최소화한다. periodic refresh와 작은 progress indicator만 사용한다.
  - monitoring screen에 의미 없는 animation을 넣지 않는다.
- imagery/iconography:
  - TUI에서는 bitmap imagery를 사용하지 않는다.
  - ASCII/Unicode symbol은 scan speed를 높이고 text fallback이 있을 때만 사용한다.

## 3. 인터페이스 계약

### 구성 요소

- 재사용할 기존 component:
  - 현재 scaffold의 CLI command output style
  - `docs/glossary.md`의 runtime status vocabulary
  - `docs/observability.md`의 observability metric group
- 새로 만들거나 바꿀 component:
  - 대화가 시작되면 사라지는 compact first-run/welcome block
  - 진단 prefix 없이 읽히는 user·assistant turn 표현
  - 바로 아래에 고정 runtime status line이 있는 대화 composer
  - 최초 실행 model picker와 managed-backend 설정 흐름
  - metric summary strip
  - model comparison table
  - session timeline
  - token budget meter
  - latency sparkline
  - gate/failure list
  - approval queue
  - log/evidence detail panel
  - export/prune dialog
- variant와 state:
  - healthy
  - degraded
  - blocked
  - collecting
  - stale
  - redacted
  - unavailable
- token/component ownership:
  - Runtime core는 data state를 소유한다.
  - TUI는 presentation과 user decision을 소유한다.
  - `docs/observability.md`는 metric schema 방향을 소유한다.

### 기본 TUI 계약

- terminal에 연결된 인자 없는 `rpotato`는 대화 controller를 연다.
- 최초 frame은 overview ledger page를 절대 render하지 않는다. Compact한 네 줄 welcome, 현재 project label, composer, status line만 보여주며 첫 turn 뒤에는 한 줄 identity header로 전환한다.
- 일반 입력은 dispatch 전에 user turn으로 표시하고, 보이는 결과는 assistant turn으로 표시한다. 오류도 inline으로 유지하되 직접 원인과 복구 action을 말한다.
- 대화 turn은 terminal display cell 기준으로 줄바꿈하고 긴 응답의 모든 line을 `/more`와 `/back`으로 확인할 수 있어야 한다.
- 상세 revision, hash, ledger count, projection freshness, workflow field, monitor table은 명시적인 status/diagnostic view에서만 볼 수 있다.
- 최초 실행에서는 같은 terminal 흐름 안에서 출처 기반 model 후보를 나열하고 확인 전에 model ID/version, quantization, download size, context limit, RAM 상태, license, 추천 근거를 보여준다.
- managed backend는 자동으로 설치하거나 재사용한다. 기본 경로에서 사용자가 `llama.cpp` executable 또는 GGUF filesystem path를 입력하게 하지 않는다.
- composer가 focus 중심이며, 바로 다음 status line은 항상 `model | ctx used/limit (%) | compaction | backend | session` 순서를 사용한다.
- Attached terminal composer는 rounded border 하나, cyan focus marker, live cursor 아래 placeholder 없는 입력 공간을 사용한다. No-color와 redirected fallback은 plain `›` prompt를 유지한다.
- Status segment는 model/focus cyan, healthy green, due/degraded yellow, failed/stale red, session/secondary muted처럼 의미별로 독립 표시하며 전체 status row를 하나의 success color로 칠하지 않는다.
- Context segment는 측정 사용량과 비율을 표시하고 공간이 허용될 때 compaction을 바로 옆에 둔다. 좁은 terminal에서는 status bar를 줄바꿈하지 않고 뒤쪽 segment부터 truncate한다.
- model과 context 값은 최신 기록된 model run, backend 상태는 managed sidecar, session은 active canonical session identity에서 읽는다. 없는 값과 stale backend 상태는 추측하지 않고 표시한다.
- `/model`, `/compact`, `/update`, `/status`, `/sessions`, `/doctor`, `/more`, `/back`, `/clear`, `/help`, `/quit`가 일반 TUI 동작을 담당한다. 기존 세부 subcommand는 `rpotato debug --help` 아래의 고급 호환 surface로 유지한다.
- ANSI attached terminal에서는 semantic color와 cursor positioning을 쓸 수 있다. Redirect된 출력, `TERM=dumb`, `NO_COLOR`에서는 안정된 plain text를 유지한다.

### 접근성

- 기준: readable contrast와 no color-only state를 가진 keyboard-first terminal accessibility
- keyboard/focus behavior:
  - 모든 action은 mouse 없이 접근 가능해야 한다.
  - focus는 monochrome terminal에서도 보여야 한다.
  - destructive action은 명시적 confirmation이 필요하다.
- contrast/readability:
  - critical value에 low-contrast dim text를 쓰지 않는다.
  - light/dark terminal theme에서 읽기 쉬워야 한다.
- screen-reader semantics:
  - CLI monitor command는 TUI summary data의 plain text equivalent를 제공해야 한다.
- reduced motion과 sensory consideration:
  - refresh interval은 설정 가능해야 한다.
  - 실패 시 화면을 flash하지 않는다.

### 반응형 동작

- 지원 breakpoint/device:
  - 최소 terminal target: 80x24
  - multi-pane layout이 가능한 wide terminal
  - single-column fallback이 필요한 narrow SSH pane
- layout adaptation:
  - wide: left navigation, top metric strip, main table, detail panel
  - medium: top tabs, summary strip, one main panel, collapsible detail
  - narrow: stacked section과 explicit drill-down screen
- touch/hover 차이:
  - hover에 의존하지 않는다.
  - mouse support는 이후 선택 기능일 수 있지만 필수이면 안 된다.

### 상호작용 상태

- loading:
  - data source, last update time, SQLite projection 또는 ledger replay 사용 여부를 보여준다.
- empty:
  - 아직 model run이 기록되지 않았음을 설명하고 다음 command를 보여준다.
- error:
  - 한국어 cause, 영향을 받은 data source, 안전한 recovery command를 보여준다.
- success:
  - 검증된 metric timestamp와 health status를 보여준다.
- disabled:
  - 빠진 backend/model/session precondition을 설명한다.
- offline/slow network:
  - monitoring은 local SQLite/ledger만으로 offline 동작해야 한다.

### 문구 원칙

- tone: 짧고 실무적인 한국어
- 용어:
  - `model run`
  - `token 사용량`
  - `context 사용량`
  - `backend health`
  - `stop gate`
  - `evidence`
  - `redacted`
- microcopy rule:
  - failure reason은 무엇이 왜 blocked되었는지 말해야 한다.
  - privacy-sensitive panel은 redacted data를 명시적으로 표시해야 한다.
  - monitoring screen 안에는 marketing copy를 넣지 않는다.

## 4. 구현 화면

### 구현 제약

- framework/styling system:
  - 현재 interactive TUI는 std-only line controller다.
  - attached-terminal frame은 bounded ANSI layout으로 status line을 composer 아래에 두고 cursor를 input line으로 되돌린다. Scripted/redirected 실행은 plain-text fallback을 사용한다.
  - 더 풍부한 full-screen TUI용 framework는 아직 선택하지 않았다.
  - SQLite projection access는 `rusqlite`를 사용한다.
  - TUI는 DB를 직접 소유하지 않고 runtime core contract를 통해 runtime state를 소비해야 한다.
- design-token constraint:
  - semantic color name만 사용한다: healthy, warning, failed, selected, muted.
  - fixed width column에는 truncation rule이 필요하다.
- performance constraint:
  - TUI는 long-running session을 monitoring하는 동안에도 반응성을 유지해야 한다.
  - SQLite read는 bounded/paginated여야 한다.
  - live update가 approval을 block하면 안 된다.
- compatibility constraint:
  - SSH/Linux server 사용은 1급 context다.
  - core monitoring에 browser requirement가 없어야 한다.
  - optional HTML은 기존 monitor query data에서 local로 생성하며 baseline operation에 필수이면 안 된다.
- test/screenshot expectation:
  - TUI smoke test는 80x24와 wide terminal size에서 수행한다.
  - 기본 frame 회귀 테스트는 raw ledger/hash/projection field가 없고 composer/status 순서가 고정됨을 증명한다.
  - 자연어 인사 회귀 테스트는 대화로 표시되고 patch proposal을 시작하지 않음을 증명한다.
  - 시각 인수 기준은 120x40 terminal capture 한 장을 2026-07-22 Codex·Claude Code 참조와 비교한다. 계약 위반이 없으면 한 번의 bounded pass로 종료한다.
  - HTML test는 browser runtime dependency를 추가하지 않고 semantic structure, escaping, privacy marker, narrow-screen layout을 검사한다.

### Monitoring TUI 화면 계약

최소 overview layout:

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

규칙:

- top strip은 항상 model, backend health, token total, throughput, memory를 보여준다.
- 모든 metric에는 timestamp 또는 stale marker가 있어야 한다.
- table은 기본적으로 운영상 가장 유용한 field로 sort한다. active run, failed/degraded, recent 순서다.
- detail panel은 raw prompt/source를 기본으로 보여주면 안 된다.
- export와 prune action은 먼저 dry-run summary를 보여줘야 한다.

### HTML Surface 위치

HTML은 local monitoring summary를 검토하고 공유하기 위한 선택형 offline snapshot이다. CLI나 TUI를 대체하지 않으며 server를 추가하지 않는다.

계약:

- TUI가 local/SSH/server context의 primary monitoring surface다.
- CLI monitor command는 plain text fallback이다.
- `rpotato monitor export --format html`은 완전한 HTML document 하나를 standard output에 기록하며 사용자는 이를 파일로 redirect할 수 있다.
- HTML은 SQLite projection과 canonical ledger가 제공하는 기존 bounded monitor query data를 사용한다. 별도의 monitoring truth source를 만들면 안 된다.
- document는 self-contained다. JavaScript, remote font, image, stylesheet, network request, local server를 사용하지 않는다.
- 제한적인 content security policy로 script, connection, form, embedding, base URL 변경을 차단한다. Inline CSS만 허용한다.
- 모든 dynamic value는 HTML escape한다. raw prompt, raw source, credential, 전체 local filesystem path를 render하지 않는다.
- report는 path를 노출하지 않고 local data source를 식별하며, 사용 가능한 최신 metric timestamp 또는 명시적인 stale/unavailable marker를 보여준다.
- semantic heading, landmark, caption, table로 읽기 쉬운 document structure를 제공한다. Status 의미는 항상 text를 포함하고 color에만 의존하지 않는다.
- light/dark color scheme를 지원한다. 좁은 화면에서는 section을 쌓고 넓은 table은 document를 자르지 않고 가로 scroll한다.
- empty, unavailable, redacted, error state는 짧고 실무적인 한국어 문구를 사용하며 report의 나머지 부분은 유지한다.
- export 생성은 read-only/offline이다. 생성된 파일을 여는 것은 사용자의 명시적인 action이다.

## 5. 열린 질문

- [ ] 더 풍부한 full-screen TUI에 Rust framework를 도입할 것인가?
- [ ] 기본 monitoring retention period는 얼마인가?
