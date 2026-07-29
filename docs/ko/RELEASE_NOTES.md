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
