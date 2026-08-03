# 릴리즈 노트

현재 릴리즈 train을 간결하게 유지하는 인덱스입니다. 과거 기록은 원문 그대로 범위별 archive에 보존합니다.

## 과거 릴리즈

- [v0.40–v0.51](releases/v0.40-v0.51.md)
- [v0.38–v0.39](releases/v0.38-v0.39.md)
- [v0.37](releases/v0.37.md)
- [v0.30–v0.36](releases/v0.30-v0.36.md)
- [v0.20–v0.29](releases/v0.20-v0.29.md)
- [v0.10–v0.19](releases/v0.10-v0.19.md)
- [v0.1–v0.9](releases/v0.1-v0.9.md)

## v0.55.1 후보 - 반복형 웹 도구 재판단

상태: 미출시 candidate

- 검증된 각 웹 observation 뒤 structured model decision에 다시 진입해 Search,
  Open, Find, Answer가 실제 제한된 loop를 이루도록 보정
- Search는 검색 결과 근거만 반환하고 Open·Find를 내부 실행하지 않으며, 각각 별도
  model 판단을 거치도록 분리
- Open은 runtime이 발견한 HTTPS URL, Find는 현재 page에만 허용하여 model 출력이
  network 권한을 넓히지 못하게 유지
- 동일 호출 반복을 거절하고 도구 3회·후속 model turn 3회 상한을 적용한 뒤 마지막
  출처 결합 observation으로 안전하게 fallback
- 순서가 있는 fake-sidecar protocol fixture로 production terminal 경계의
  Search→Open→Find→Answer와 malformed·반복 호출 안전 종료를 검증

## v0.55.0 - 모델 기반 생성과 신뢰 가능한 터미널 여정

릴리즈 날짜: 2026-08-03

이 minor 릴리즈는 공개하지 않은 v0.54.0 모델 기반 생성 이정표와 v0.55.0
터미널 완성도 작업을 합칩니다. 작은 제품 고정 생성 상한을 제거하고 기본 TUI
사용자 여정 전체를 강화하지만, 비교 대상 제품과 동등하다고 주장하지 않습니다.

### 포함한 것

- 생성, 첨부, 압축, 웹, agent와 TUI budget을 제품 고정 상한 대신 활성 모델의
  context와 남은 capacity에서 계산
- 정상 `stop`만 완결 답변으로 인정하고 `length`와 알 수 없는 finish를 미완결로 처리
- 정상 typed model 판단을 보존하고 잘못된 소형 모델 출력에만 제한된 복구 적용
- 후속 질문과 `/resume`에서 이전 작업을 다시 실행하지 않고 typed web activity 복원
- 순서가 있는 source evidence와 고정 용량 network worker pool을 사용하는 취소 가능한
  Search→Open→Find→Answer 조사
- 요청 단계·경과 시간, 활성 `Ctrl+C` 취소, 확장된 Markdown, 입력 내용을 보존하는
  scroll과 실패 요청 첨부 복구
- cache된 language model과 lazy vision projector 준비 상태를 분리하고 활성 backend에
  연결된 projector 검증
- native-terminal 회귀 사용자 여정과 별도 opt-in 관리형 실제 모델 smoke

### 호환성과 경계

- 기존 command, 저장 session, model cache와 GitHub Releases 전용 배포는 호환됩니다.
- 기본 CI는 production 경계를 지나는 결정적 동작을 검증합니다. Opt-in 실제 모델
  smoke는 의도적으로 범위가 좁으며 광범위한 모델 품질·하드웨어 호환성이나 Codex,
  Claude Code, OpenCode와의 동등성을 증명하지 않습니다.
- 제한 없는 tool orchestration, remote plugin source, MCP와 외부 app 연동은 지원
  범위 밖에 있습니다.

## v0.53.0 - 제한된 런타임 아키텍처와 자동 근거 수집

릴리즈 날짜: 2026-07-29

이 minor 릴리즈는 저장소 전체의 책임 분리를 완료하고, 최신성이 필요한 질문을
소형 로컬 모델의 오래된 기억보다 먼저 runtime-owned web grounding 경로로
보냅니다. Public CLI와 TUI command surface는 호환됩니다.

### 포함한 것

- 특정 제품명·행사명·답변 모양 예외를 generic freshness feature, typed query
  plan, evidence coverage와 answer binding으로 교체
- 로컬 저장소 질문과 명시적 no-web 요청은 offline 경로에 유지하면서 volatile
  fact, 현재 버전과 경험적 비교를 제한된 `WebSearch`·`WebOpen`·`WebFind`로 routing
- 생성 답변과 fallback grounded answer의 runtime source attribution을 보존하고
  stale·conflicting·unrelated evidence를 차단
- 현재 model identity 질문은 runtime 설정에서 답하되 model 비교·추천 요청은
  가로채지 않음
- 일반 active conversation은 오래된 historical source pointer를 허용하되 명시적
  `/resume`은 잘못된 artifact를 조용히 버리지 않고 오류로 보고
- 대형 TUI, context, model, backend, install, workflow, patch, collaboration,
  web, CLI, persistence, terminal과 test owner를 명시적 dependency·migration
  contract를 가진 bounded module로 분리
- 과거 release note와 실행 회고를 bounded archive로 나누면서 machine-verifiable
  ownership coverage를 유지

### 호환성과 경계

- 기존 public command, 저장 session, model artifact, lazy projector, 관리형
  backend 동작과 GitHub-only 배포는 호환됩니다.
- Web evidence는 제한된 읽기 전용 untrusted input이며 browser, shell,
  filesystem write, credential 또는 approval 권한을 주지 않습니다.
- 이 리팩터링은 새로운 model quality, benchmark, memory 또는 hardware 결과를
  주장하지 않습니다.

## v0.52.1 - 관리형 llama.cpp 구조화 대화 복구

릴리즈 날짜: 2026-07-29

이 patch는 관리형 `llama.cpp` backend의 기본 TUI 대화 경로를 복구합니다.
v0.52.0의 structured answer schema가 pinned compiler의 허용 범위를 넘는 grammar
repetition을 요청하여 간단한 인사도 generation 전에 HTTP 400으로 실패했습니다.

### 포함한 것

- Grammar 수준의 `answer.maxLength` repetition은 제거하고 runtime의 독립적인
  16 KiB visible-answer 검증은 유지
- 지원하지 않는 string·array·object repetition bound를 generation lifecycle
  시작과 backend request 전송 전에 차단
- JSON Schema의 실제 subschema 위치만 순회하여 `maxLength`, `minItems` 같은
  이름의 정상 출력 property를 허용
- 관리형 grammar의 검증된 1,999/2,000 경계, property-name collision,
  production turn schema와 native structured TUI request를 회귀 테스트로 보호
- Request body, response format, schema 또는 chat template 변경 시 pinned
  `llama-server`와 설치된 지원 model의 실제 smoke를 요구

### 호환성과 경계

- 기존 model, session, projector, web tool, browser 제한과 public command는
  계속 호환됩니다.
- 애플리케이션은 16 KiB보다 큰 visible structured answer를 계속 거절하며,
  호환되지 않던 grammar repetition만 제거했습니다.
- GitHub Releases만 지원하는 binary 배포 channel입니다.

## v0.52.0 - 소형 모델용 구조화 Tool Turn

릴리즈 날짜: 2026-07-28

이 minor 릴리즈는 소형 로컬 모델의 자유 형식 web-tool 지시를 schema로 제한된
decision·observation loop로 교체합니다. Tool 요청부터 검증된 evidence와 최종
visible answer까지의 전이는 model text가 아니라 runtime이 소유합니다.

### 포함한 것

- 일반 대화 model turn마다 `Answer`, `WebSearch`, `WebOpen`, `WebFind`,
  `ContinueLocal` 중 하나의 제한된 JSON schema 결정을 요구
- `WebSearch`·`WebOpen`·`WebFind`를 동일한
  `ToolCall → Observation → Answer` lifecycle로 실행하고 raw tool report를 최종
  model answer로 표시하지 않음
- Runtime-owned bounded observation을 별도 model 호출에 전달한 뒤에만 최종 답변
  생성
- 최근 user request만 사용해 후속 search query를 만들고 model response,
  attachment, credential, private value와 무관한 이전 topic을 제외
- 잘못된 structured decision은 private protocol text로 표시하지 않고 안전한 local
  continuation으로 처리
- Request가 저장소, project, source, file 또는 code 범위를 명시하지 않으면 일반
  분석 질문을 repository inspection으로 오분류하지 않음
- Search, open, find, query sanitation과 runtime request support를 bounded owner로
  분리하고 architecture·native-terminal test로 보호
- 같은 candidate의 label·ready event가 서로 취소하지 않도록 하면서 같은 event의
  새 commit supersession은 유지

### 호환성과 경계

- 기존 model, session, projector, web grounding, browser 제한과 public command는
  계속 호환됩니다.
- Web observation은 읽기 전용이며 제한된 신뢰하지 않는 evidence입니다. Browser,
  shell, filesystem write, credential 또는 approval 권한을 얻지 못합니다.
- GitHub Releases만 지원하는 binary 배포 channel입니다.
