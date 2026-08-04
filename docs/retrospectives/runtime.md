# Runtime과 제품 동작 회고

Backend, TUI, session, model capability, web grounding 등 실제 제품 동작과 runtime 경계에서 발생한 반복 가능한 실패를 기록합니다.

## 2026-07-28: Structured schema가 실제 llama.cpp grammar 한도를 넘김

### 증상

- v0.52.0 설치본에서 기본 TUI를 열고 `안녕`만 입력해도
  `/v1/chat/completions`가 HTTP 400을 반환했습니다.
- Backend health와 model loading은 정상이라 사용자는 요청이 왜 거절됐는지 status
  code만 보고 구분할 수 없었습니다.

### 원인

- Structured turn의 `answer.maxLength`를 16384로 설정했습니다. Pinned managed
  llama.cpp b9982는 이를 grammar의 `char{0,16384}`로 변환한 뒤 repetition이 sane
  limit을 넘었다며 sampler 초기화 전에 요청을 거절했습니다.
- 같은 backend를 대상으로 한 로컬 probe에서는 repetition 1999까지 HTTP 200,
  2000부터 HTTP 400이 재현됐습니다.
- Unit test는 request JSON에 `response_format`이 포함되는지만 확인했고 native TUI
  E2E의 fake sidecar는 schema를 실제 grammar로 compile하지 않아 production
  incompatibility를 정상 응답으로 통과시켰습니다.

### 재발 방지

- llama.cpp adapter가 JSON schema의 string·array·object repetition bound를
  schema 위치를 구분해 재귀적으로 검증하고 managed grammar 상한을 넘는 요청은
  generation lifecycle 시작과 network 전송 전에 차단합니다.
- Production structured turn의 `answer`에는 grammar repetition을 만들지 않는
  unbounded string schema를 사용하고, 애플리케이션 parser의 16 KiB answer bound는
  별도로 유지합니다. 실제 managed backend에서 이 schema가 compile되고 visible
  answer를 반환하는지 검증합니다.
- llama.cpp request body, `response_format`, JSON schema 또는 chat template을
  바꾸면 fake sidecar만으로 release candidate를 만들지 않습니다. Pinned managed
  backend와 설치된 지원 model에서 기본 structured 대화가 visible answer까지
  완료되는 user-journey smoke를 수행합니다.

## 2026-07-26: macOS PTY가 선택 알림 직후의 다음 명령을 잃음

### 증상

- PR candidate의 macOS `native_terminal::full_adapter`가 workflow 선택 알림은
  확인했지만 바로 뒤에 보낸 approve 명령의 Enter를 처리하지 못해 picker 대기에서
  timeout됐습니다.
- 캡처에는 approve 문자열이 composer에 완성돼 있었지만 다음 화면 전환은 없었습니다.

### 원인

- PTY 테스트가 `선택: <workflow>` 알림을 다음 입력 준비 완료 신호로 사용했습니다.
  Full-screen redraw가 느린 runner에서는 선택 알림과 새 composer 준비 사이에 다음
  명령이 도착할 수 있었습니다.
- Composer prompt 문자는 입력 모드가 활성화되기 전에 먼저 그려질 수 있었습니다.
  화면에 `›`가 보이는 것만으로 event loop의 입력 준비를 판정해 다음 명령 전체가
  사라지는 경우가 남았습니다.
- 새 composer까지 기다린 뒤에도 보이는 명령과 LF를 한 번의 PTY write로 보냈습니다.
  macOS runner에서는 명령 문자열은 모두 렌더링됐지만 마지막 LF가 Enter로 처리되지
  않았습니다. 실제 키보드 Enter가 보내는 CR과 테스트 입력 계약이 달랐습니다.
- 긴 approve 식별자를 키보드 입력처럼 한 byte씩 처리하게 해 매 byte마다 full-screen
  redraw가 발생했습니다. 느린 runner에서는 제한 시간 동안 명령 일부만 소비했으며,
  실제 사용자의 긴 문자열 붙여넣기와 다른 부하를 만들었습니다.

### 재발 방지

- 선택형 PTY 흐름은 선택 알림만 기다리지 않고, 동일 capture 구간에서
  `선택 알림 → bracketed-paste 입력 모드 활성화` 순서를 확인한 뒤 다음 명령을
  보냅니다. Prompt glyph는 화면 상태일 뿐 input-readiness marker로 사용하지 않습니다.
- 보이는 대화형 명령은 문자열이 composer에 완전히 렌더링된 것을 확인한 다음,
  실제 키보드와 같은 CR을 별도 write로 보내 제출합니다. 명령과 LF를 한 번에
  보내는 방식으로 full-screen TUI readiness를 추측하지 않습니다.
- Live terminal의 긴 명령 입력은 bracketed-paste로 한 번에 전달하고 paste 결과가
  렌더링된 뒤 CR을 보냅니다. 단순 line 모드는 기존 입력 경로를 유지해 두 모드를
  각각 검증합니다.
- Runner 지연을 가리기 위해 timeout만 늘리지 않고, 다음 입력을 받을 수 있는 의미
  readiness marker를 순서 계약으로 검증합니다.

## 2026-07-26: 과거 source hash 변경이 무관한 새 요청을 차단함

### 증상

- 장기 TUI session에서 과거 대화가 참조한 파일이 변경된 뒤, 해당 파일과 무관한
  새 코딩·일반 요청도 `ontology source reread 차단`으로 종료됐습니다.
- 전체 unit·candidate 테스트는 통과했지만 실제 사용자 session에서는 기본 요청을
  계속할 수 없었습니다.
- 대화 transcript가 ledger head를 전진시킨 뒤에는 안전한 current-state ancestor까지
  stale로 오판해 별도 세션 `/resume` 선택도 차단됐습니다.

### 원인

- 일반 대화 history와 명시적인 workflow/session resume이 같은 strict source
  reread 정책을 사용했습니다.
- Unit과 integration 테스트가 stale 과거 pointer로 새 요청 전체를 차단하는 동작을
  정상 계약으로 고정했습니다.
- Fresh fixture 중심 E2E에는 `요청 성공 → 파일 변경 → 무관한 다음 요청` 장기
  session 시나리오가 없었습니다.
- 세션 테스트는 controller mock과 빈 transcript 위주여서 실제 ledger suffix,
  명시적 선택, 선택한 transcript 재수화를 한 흐름으로 검증하지 않았습니다.

### 재발 방지

- 일반 대화 history는 best-effort context hint로 취급하고 stale source pointer만
  제외합니다. Transcript 대화와 현재 요청의 최신 repository context는 유지합니다.
- 명시적인 `resume`/`continue`, patch 적용, 현재 source claim은 strict hash 검증을
  유지합니다. 두 정책을 같은 함수 호출 경로로 암묵적으로 합치지 않습니다.
- 장기 session E2E는 파일 변경 뒤 무관한 다음 요청 성공과 backend 재호출을
  검증하며, 기존 테스트가 사용자 기대와 반대인 실패를 보호하지 않는지 확인합니다.
- Current-state 선택 lease는 검증된 `transcript.recorded` suffix만 안전한 ancestor로
  허용합니다. 세션·워크플로 상태 변경 suffix는 계속 fail-closed로 차단합니다.
- 기본 TUI는 저장된 대화를 자동 복원하지 않습니다. `/resume` 선택 후 해당 세션만
  복원하는 실제 runtime·PTY 회귀를 함께 유지합니다.

## 2026-07-25: vision capability와 readiness를 boolean으로 합쳐 TUI가 거짓 안내함

### 증상

- Qwen과 Gemma 모두 source-backed projector가 선언되어 있는데 projector를 아직
  적재하지 않은 text-ready backend가 `vision text-only`로 표시됐습니다.
- 사용자가 비전 상태를 물으면 제품 사실을 조회하지 않고 local model이 “rpotato는
  비전을 지원하지 않는다”고 추측해 답했습니다.
- 캐시된 모델로 전환해도 선택·진행 화면은 항상 “다운로드하고 적용”과 “다운로드
  중”으로 표시했습니다.

### 원인

- `vision_ready: bool` 하나가 모델의 capability와 현재 backend readiness를 동시에
  표현해 `지원하지만 지연 준비됨` 상태를 표현할 수 없었습니다.
- Setup mock은 production이 항상 `false`로 만들던 값을 `true`로 반환해 실제 기본
  경로와 반대 상태를 검증했습니다.
- Cache 재사용 테스트는 filesystem fetch 함수에만 있었고 `/model`의 선택→확인→진행
  사용자 흐름과 표시 문구에는 연결되지 않았습니다.

### 재발 방지

- Optional capability는 `ready`, `on-demand`, `unsupported`, `unavailable`처럼
  capability와 readiness를 보존하는 상태로 표현하며 boolean으로 축약하지 않습니다.
- Runtime capability 질문은 model 생성에 맡기지 않고 manifest·registry·backend의
  검증된 local fact로 답합니다.
- 모델 선택 회귀는 fresh download와 cache hit를 모두 실제 production 기본 상태로
  검증하고, 언어 모델과 lazy projector의 cache·download 용량을 분리해 표시합니다.

## 2026-07-24: v0.49.0 vision manifest가 기존 text-only 설치를 차단함

### 증상

- v0.48.1에서 정상 사용하던 `gemma-4-e4b` 기본 모델로 v0.49.0 TUI를 시작하면
  `vision projector registry binding이 준비되지 않았습니다` 경고가 표시됐습니다.
- 기존 backend는 `4096` context로 남았고 일반 모델 요청도 runtime readiness 단계에서
  차단될 수 있었습니다.

### 원인

- Registry parser는 schema v1을 `unavailable-legacy` text-ready 상태로 정상 수용했지만,
  configured runtime spec은 manifest에 projector가 선언됐다는 이유만으로
  `visionStatus=ready`와 exact mmproj binding을 모든 요청에 요구했습니다.
- Candidate 검증은 fresh schema v2와 projector 경로 중심이었고, 직전 릴리스에서 실제로
  만들어진 schema v1 default selection·registry 조합을 재사용한 업그레이드 회귀가
  없었습니다.

### 재발 방지

- Optional capability는 base capability의 시작·대화·context reconciliation을 차단하지
  않으며 text와 vision runtime requirement를 분리합니다.
- Vision projector는 이미지 요청에서만 검증·cache reuse·download recovery를 수행하고,
  성공한 뒤 기존 registry evidence를 보존한 schema v2 binding을 원자적으로 기록합니다.
- Model schema·manifest·runtime artifact 계약 변경은
  `scripts/ci/verify-model-upgrade-compatibility.sh`의 legacy fixture, manifest context,
  projector cache와 registry migration 회귀를 candidate preflight에서 통과해야 합니다.

## 2026-07-22: v0.45.0 TUI가 혼합 세대 ledger를 거부함

### 증상

- 과거 버전부터 사용한 data home에서 `rpotato`를 실행하면 기본 TUI가 시작되지 않고
  `runtime ledger read-only view는 chained event만 허용합니다.` 오류가 출력됐습니다.
- canonical ledger에는 정상적인 schema v1 legacy prefix 뒤에 schema v2 chained suffix가
  있었으며 데이터 손상은 없었습니다.

### 원인

- canonical 전체 검증기는 legacy prefix와 chained suffix의 조합을 지원했지만 TUI의
  bounded read-only reader는 반환 대상 event를 모두 chained event로 제한했습니다.
- 기본 overview가 최근 80개를 요청할 때 chained suffix가 80개보다 짧으면 정상 legacy
  event가 반환 범위에 들어와 시작을 차단했습니다.
- upgrade용 native terminal fixture는 legacy current-state 이동만 검사하고 동일 data
  home에 누적된 mixed-generation ledger를 재현하지 않았습니다.

### 재발 방지

- canonical writer/validator가 지원하는 ledger 세대 전환 규칙을 read-only reader도
  동일하게 적용하고, 전체 prefix를 byte budget 안에서 확인할 수 있을 때 legacy digest와
  첫 chained predecessor를 검증합니다.
- 기본 TUI 회귀 테스트에는 반환 event budget보다 legacy prefix와 chained suffix의 합이
  큰 혼합 ledger를 사용해 실제 업그레이드 tail 경계를 고정합니다.
- 상태 형식 또는 persistence 호환성을 변경한 릴리스는 fresh fixture뿐 아니라 지원 중인
  이전 schema의 누적 상태로 무인자 TUI 진입을 검증합니다.

## 2026-07-22: v0.44.0 기본 TUI가 다른 프로젝트의 전역 상태에 차단됨

### 증상

- `rpotato`를 이전 초기화 디렉터리와 다른 프로젝트에서 실행하면 기본 TUI가 시작되지
  않았습니다.
- 실제 `current-state` 프로젝트 binding 오류가 응답 언어 검증기로 전달되어
  “응답 언어 검증 실패”라는 무관한 메시지로 바뀌었습니다.

### 원인

- 선택된 session/workflow pointer를 프로젝트별로 저장하지 않고 app data의 전역
  `current-state.json` 하나에 저장했습니다.
- native TUI smoke가 동일 프로젝트의 재실행만 검사하고, 같은 data home을 공유하는
  서로 다른 두 프로젝트의 순차 실행을 검사하지 않았습니다.
- 모델 응답에만 적용해야 할 한국어 검증기를 최상위 `AppError` 출력에도 적용했습니다.

### 재발 방지

- `current-state` pointer와 해당 lock/temp/backup은 프로젝트의 `.rpotato/state/`에
  격리하고, 기존 전역 pointer는 현재 프로젝트와 binding이 일치할 때만 보존 복사합니다.
- 기본 TUI 회귀 테스트는 같은 data home에서 프로젝트 A→B→A 전환과 기존 전역
  pointer가 다른 프로젝트를 가리키는 upgrade 상황을 포함합니다.
- 응답 언어 검증은 모델 생성 결과에만 적용하고 CLI/system 오류는 원래 메시지와
  exit code를 보존합니다.
- persistence root를 옮길 때는 accessor 호출뿐 아니라 crash-matrix가 backup directory를
  직접 열어 검사하는 경로까지 검색하고, 해당 fault-injection 테스트를 targeted gate에
  포함합니다. 이번 candidate CI는 이 오래된 backup 경로 assertion 때문에 한 번
  실패했습니다.

## 2026-07-22: bordered composer가 exact outcome 출력을 오염

### 증상

- Linux candidate에서 conversation notice의 새 시각적 들여쓰기가 exact outcome
  계약에 포함되어 원문 비교가 실패했습니다.
- Windows ConPTY에서는 composer status 줄과 뒤이은 terminal fault 첫 줄이 결합되어
  `terminal.capability.mode-read` 결과가 오염됐습니다.

### 원인

- 새 composer가 status 줄을 개행으로 닫지 않은 채 cursor를 입력 줄로 되돌렸습니다.
- Exact outcome capture 정규화가 기존 `notice:` 형식만 알고 새 `◇`와 continuation
  gutter를 presentation metadata로 제거하지 못했습니다.

### 재발 방지

- ANSI composer는 status 줄을 개행으로 닫은 뒤 전체 composer 높이만큼 cursor를
  복귀시켜 후속 stderr/outcome 출력의 줄 경계를 보존합니다.
- Exact outcome 검증은 알려진 presentation prefix만 제거하고 정본 outcome 본문은
  byte-exact하게 계속 비교합니다.
- Default TUI 구조 변경 candidate는 전체 CI 전에 interactive recovery outcome과
  native terminal adapter targeted 테스트를 함께 실행합니다.

## 2026-07-22: Windows ConPTY에 Unix식 byte 단위 line editor를 적용함

### 증상

- Windows candidate의 compile과 install lifecycle은 통과했지만 native terminal
  `entry_quit`가 ConPTY child timeout으로 실패했습니다.
- 일반 TUI 입력에서 console line mode를 끈 뒤 Rust stdin의 byte 단위 read가 입력을
  완료하지 못해 `quit`가 controller까지 전달되지 않았습니다.

### 원인

- macOS/Linux PTY에서 검증한 raw byte line editor를 Windows console에도 같은 방식으로
  적용할 수 있다고 가정했습니다.
- Windows console의 `ReadConsoleW`/input record 경계와 UTF-16 입력 계약을 구현하지 않은
  상태에서 `ENABLE_LINE_INPUT`만 제거했습니다.

### 재발 방지

- Windows는 전용 console event reader가 native ConPTY 테스트로 검증되기 전까지 기존
  canonical line input을 유지합니다.
- Unix live slash palette 테스트는 Unix로 범위를 명시하고, Windows terminal 변경은
  exact candidate의 `entry_quit`와 `full_adapter`를 필수 회귀 계약으로 둡니다.
- Windows 조건부 코드는 compile 성공만으로 완료하지 않고 native terminal job의 실제
  입력·종료 결과를 확인합니다.

## 2026-07-23: projector cache-key 변경과 기존 경로 계약 테스트 불일치

### 증상

- Projector partial을 SHA revision별로 분리했지만 기존 model adapter 테스트는
  revision 없는 과거 파일명을 계속 기대해 candidate 전체 테스트에서 한 건이
  실패했습니다.

### 원인

- 새 projector 전용 회귀 테스트와 setup 테스트만 실행하고, 같은 경로 helper를
  직접 검증하는 기존 model adapter 테스트를 targeted 범위에 포함하지 않았습니다.

### 재발 방지

- Artifact 또는 cache key를 변경하면 새 동작 테스트뿐 아니라 해당 path helper를
  직접 호출하는 기존 테스트를 `rg`로 찾아 함께 실행합니다.
- Revision-aware 경로 계약은 고정 문자열을 중복하지 않고 manifest SHA prefix로
  기대값을 계산해 manifest 갱신과 테스트가 어긋나지 않게 합니다.

## 2026-07-24: resume summary 확장과 기존 lifecycle 문자열 계약 불일치

### 증상

- Model별 context window를 resume summary에 추가한 뒤 targeted context test와
  architecture contract는 통과했지만, PR 전체 test에서 durable resume lifecycle
  한 건이 이전 `transcript turns=3` 인접 문자열을 계속 기대해 실패했습니다.
- 실제 resume 동작은 `context limit=1024`, `transcript turns=3`, backend 재호출 없음,
  source pointer 복원을 모두 보존했고 출력 계약만 확장된 상태였습니다.

### 원인

- Runtime report field를 추가하면서 해당 summary를 최종 사용자 출력으로 검증하는
  기존 patch-loop lifecycle test를 변경 영향 범위에 포함하지 않았습니다.
- 새 model-window 단위 테스트가 값 계산은 고정했지만, 그 값이 resume report에
  합성되는 통합 문자열 계약까지 대신 검증한다고 잘못 판단했습니다.

### 재발 방지

- `ResumeContext::summary`의 field나 순서를 바꾸면 호출 지점을 `rg`로 찾고
  `continue`·`resume` lifecycle의 exact targeted test를 candidate 전에 실행합니다.
- Durable resume test는 context limit과 transcript turn 수를 함께 검증하고,
  실패 시 실제 report를 assertion message로 남겨 기능 결함과 기대 문자열 불일치를
  한 번에 구분합니다.

## 2026-07-26: fresh TUI와 best-effort history 경계를 일부 명령·삭제 상태가 우회함

### 증상

- 화면은 `session new`였지만 첫 입력이 `/compact`이면 이전 durable session의
  transcript를 대상으로 압축을 시도할 수 있었습니다.
- 과거 source가 수정된 경우는 무관한 다음 요청에서 제외됐지만, 같은 source가
  삭제되면 current ontology와 historical reread 경로가 요청 전체를 차단했습니다.

### 원인

- 새 세션을 lazy 생성하는 경계를 요청·첨부에만 적용하고 context mutation 명령에는
  적용하지 않았습니다.
- Best-effort source 정책을 hash mismatch 한 상태로만 모델링해 `NotFound`와
  non-file 상태를 별도 stale 상태로 분류하지 않았습니다.
- Stale ontology를 filesystem scan으로 우회하지 않는 보안 규칙을 요청 전체 실패로
  구현해, source 없는 일반 답변까지 불필요하게 막았습니다.

### 재발 방지

- Fresh TUI에서 현재 session을 읽거나 변경하는 모든 명령은 먼저 새 session을
  확정하며, 이전 session을 암묵적으로 대상으로 삼지 않습니다.
- Historical source는 변경·삭제·non-file을 `dropped`로 처리하되 malformed pointer,
  project boundary 위반, permission·기타 I/O 오류는 계속 fail-closed로 유지합니다.
- Stale ontology는 filesystem fallback으로 우회하지 않고 빈 source context로
  격리합니다. Patch·명시적 reread 경계의 current source 검증은 계속 strict하게
  유지합니다.
- 변경과 삭제를 각각 실제 두 요청 lifecycle로 검증하고, fresh `/compact`가 기존
  session id를 유지하지 않는 회귀 테스트를 둡니다.

## 2026-07-26: session resume 분리 뒤 ledger lock과 Windows fault probe 계약이 남음

### 증상

- Selection lease가 transcript-only ledger suffix를 허용하도록 바꾼 뒤 active
  workflow를 읽는 경로에서 ledger writer guard를 잡은 채 같은 ledger를 다시 읽어
  self-block했고, 전체 unit test에서 후속 40건이 연쇄 실패했습니다.
- Windows native-terminal test는 이제 local conversation transition인 session
  resume를 workflow dispatch fault probe로 계속 사용해 존재하지 않는
  `세션 선택 확인` picker를 기다렸습니다.

### 원인

- Ledger events와 binding의 일관된 snapshot만 필요했지만 guard의 수명을 active
  workflow validation까지 넓혔습니다.
- Session resume와 workflow resume를 제품 코드에서는 분리했지만 플랫폼별 fault
  fixture의 dispatch 의미까지 함께 갱신하지 않았습니다.

### 재발 방지

- Ledger·filesystem lease는 필요한 snapshot을 만든 직후 scope에서 해제하고,
  다른 adapter나 workflow loader를 호출하는 동안 보유하지 않습니다.
- Candidate preflight는 active workflow가 있는 current-state lease가 ledger guard를
  해제한 뒤 다시 읽을 수 있는 exact 회귀 테스트를 실행합니다.
- Workflow pre/post-dispatch fault test는 실제 `TuiIntent` workflow action만
  사용합니다. Session resume 확인 picker를 dispatch probe로 재도입하면 architecture
  contract가 차단합니다.

## 2026-07-27: 누적 대화량과 웹 근거 한도를 단일 호출·hard failure로 모델링함

### 증상

- TUI의 `ctx`가 여러 대화 뒤에도 마지막 model run의 prompt token 수에 머물러
  실제 보존된 대화가 증가하는 모습을 표시하지 못했습니다.
- 웹 근거가 8 KiB에 도달하면 이미 확보한 검색 결과와 원문까지 버리고 요청 전체를
  실패시켰습니다. 실패한 사용자 요청과 runtime 오류도 transcript에 남지 않아
  다음 질문이 오류의 맥락을 알 수 없었습니다.
- `내 이름이 뭐였지?` 같은 문맥 의존 질문을 agent 정체성 질문으로 오분류해
  정상적인 대화 회상을 고정 응답이 가렸습니다.

### 원인

- 상태 표시를 durable conversation이 아니라 observability의 최신 단일 model run에
  결합했습니다.
- Evidence byte budget을 수집 중단 경계가 아닌 research terminal state로 사용했고,
  한국어처럼 UTF-8 byte 폭이 큰 자료와 여러 출처를 포함한 fixture가 없었습니다.
- 성공한 user/model pair만 대화로 보존하고 runtime-owned 실패 결과를 버렸으며,
  agent identity shortcut이 명시적 2인칭 표현 없이도 동작했습니다.

### 재발 방지

- Idle `ctx`는 현재 session의 보존된 완료 대화와 첨부를 기준으로 계산하고, 진행 중
  예상치와 동일한 estimator를 사용합니다. 0보다 큰 사용량은 정수 반올림 때문에
  `0%`로 표시하지 않습니다.
- 웹 evidence budget 소진은 soft truncation으로 처리합니다. 새 근거 수집은 멈추되
  이미 확보한 snippet·원문·검증 URL로 최종 답변 또는 출처 fallback을 만듭니다.
  Supporting passage도 같은 byte budget을 통과시킵니다.
- 실패한 user request와 bounded runtime error를 canonical transcript pair로 남겨
  후속 prompt와 웹 최종 합성에 `runtime` 역할로 전달합니다.
- Agent 정체성 shortcut은 `너·네·your`처럼 발화 대상을 명시한 질문에만 적용합니다.
  문맥 의존 이름·대명사 질문은 conversation memory를 거치는 회귀 테스트와 실제
  promoted model smoke로 검증합니다.
- Candidate 전에는 긴 한국어 근거의 soft truncation, 실패 turn 복원, 누적 `ctx`
  증가, 실제 검색 뒤 no-web 후속 답변을 각각 검증합니다.

## 2026-07-28: 후속 웹 검색과 저장소 탐색을 동일한 동사로 분류함

### 증상

- `검색해봐`, `찾아보라고` 같은 일반 웹 후속 요청이 이전 대화 주제를 잃고 현재
  문장 자체를 검색했습니다.
- `찾아`라는 동사만으로 coding-agent 경로에 진입해, 월드컵 우승국을 묻는 일반
  질문이 `repo-map`의 파일 근거 요구로 차단됐습니다.

### 원인

- 외부 검색 query를 현재 문장의 literal substring으로만 제한해, 모델이 최근 사용자
  발화를 자립형 검색어로 투영할 수 없었습니다.
- 의미 분류 테스트가 단일 문장 route만 검증하고, 실제 사용자 대화의 연속된
  질문·정정·검색 요청을 재현하지 않았습니다.

### 재발 방지

- 일반적인 `검색·찾아·분석` 동사는 conversation에 남기고, 파일·코드·저장소처럼
  명시적인 local scope가 함께 있을 때만 `repo-map`으로 분류합니다.
- 후속 검색어는 현재 주제와 연결된 최근 사용자 발화 사슬만 사용해 자립형 query로
  만들며, 무관한 과거 발화·모델 답변·첨부 내용은 외부 query provenance에서
  제외합니다. 연도 같은 후속 한정자는 모델 query에서 누락돼도 보존합니다.
- 대화형 검색 변경은 단일 문장 unit test에 더해 `주제 질문 → 시점 후속 질문 →
  검색 요청 → 사용자 정정`의 실제 turn sequence를 회귀 테스트로 고정합니다.

## 2026-07-28: 일반 분석과 저장소 분석의 분류 경계가 오래된 테스트 입력들과 충돌함

### 증상

- 전체 candidate test에서 source inspection 계획 테스트가 `inspect-sources` 대신
  `answer-only`를 받아 실패했습니다.
- 첫 fixture를 수정한 다음 candidate에서는 workflow performance E2E의
  `src/lib.rs 구조를 분석해줘` 입력이 같은 이유로 `answer-only`가 되어, 읽기 전용
  action 계약에서 차단됐습니다.
- 두 실패 모두 단독 실행에서 재현됐고, 나머지 unit test와 Windows·macOS
  candidate 검증은 통과했습니다.

### 원인

- 일반 질문의 `검색·찾아·분석` 동사를 저장소 탐색으로 오분류하지 않도록
  repository scope를 필수화했지만, 기존 source inspection 테스트는 `구조 분석해줘`
  라는 일반 문장으로 저장소 경로를 기대했습니다.
- Candidate preflight가 분류 unit 경계만 실행하고 실제 `rpotato run` 성능 E2E는
  실행하지 않아 두 번째 stale fixture가 전체 CI 끝에서 발견됐습니다.

### 재발 방지

- Source inspection 테스트는 `이 저장소 구조 분석해줘`처럼 repository scope를
  명시하고, 일반 `구조 분석해줘`는 conversation의 `answer-only`로 남는 별도 회귀
  테스트로 고정합니다.
- Workflow performance E2E도 `이 저장소의 src/lib.rs 구조를 분석해줘`처럼 범위를
  명시합니다.
- Candidate preflight와 release workflow contract에 repository-scoped read-only
  계획 unit과 workflow performance E2E를 함께 추가해, 분류기 변경 뒤 stale
  실행 fixture가 전체 CI에서 처음 발견되지 않게 합니다.

## 2026-07-29: 유효한 소형 모델 판단이 필수 웹 근거 수집을 우회함

### 증상

- 최신 경기 결과와 모델 성능 비교처럼 외부 근거가 필요한 질문에도 소형 모델이
  구조적으로 유효한 `answer`를 선택하면 자동 검색 없이 기억에 의존해 답했습니다.
- 검색이 실행된 뒤에도 보강어 `official`을 페이지 내부 검색어로 선택해, 사용자가
  요구한 `우승` 문장을 놓치고 문서 제목만 답으로 반환했습니다.

### 원인

- 도구 판단 JSON의 형식 유효성과 답변의 근거 충분성을 같은 것으로 취급했습니다.
- 회귀 테스트가 모델이 `search`를 선택한 경로만 검증하고, 유효하지만 부적절한
  `answer`를 runtime 정책이 덮어써야 하는 경로와 실제 TUI 응답 품질을 고정하지
  않았습니다.

### 재발 방지

- 시의성 높은 결과·현재 버전·경험적 성능 비교는 소형 모델 판단보다 먼저
  deterministic grounding policy를 적용하고, 명시적 no-web 및 로컬 범위는 별도로
  보존합니다.
- 검색 query 보강어와 페이지 내부 핵심어를 분리합니다. `WebFind`는 질문이 요구한
  값의 핵심어를 우선하며, 답변이 그 값을 직접 제공하거나 미확인을 명시하지 않으면
  안전한 grounded fallback으로 교체합니다.
- 자동 검색 변경은 unit test에 더해 실제 TUI에서 `모델 정체성 → 시의성 질문 →
  경험적 비교`를 입력해 무수동 검색, 직접 답변, 출처 결합을 한 번 확인합니다.

## 2026-08-04: 로컬 접근 안내와 실제 TUI 실행 경로가 분리됨

### 증상

- 사용자가 로컬 파일 접근 가능 여부를 물으면 제품은 접근할 수 없다고 답했고,
  재실행 후에도 프로젝트 파일 읽기 요청을 실제로 수행하지 못했습니다.
- 로컬 도구 schema와 executor의 일부 단위 계약은 있었지만 일반 TUI 요청은 해당
  실행 loop가 아니라 기존 workflow 보고 경로로 빠졌습니다.
- 실제 파일 근거를 포함한 답변의 경로와 snake_case identifier가 한국어 projection
  과정에서 잘려, 도구 실행이 성공해도 visible answer가 훼손될 수 있었습니다.

### 원인

- Capability를 protocol type, executor, production request routing, persistence,
  사용자 화면을 잇는 end-to-end 기능이 아니라 독립된 구성 요소의 존재로 판단했습니다.
- Native terminal 검증이 로컬 도구 호출, observation 재주입, 두 번째 도구 호출,
  최종 답변으로 이어지는 실제 사용자 여정을 포함하지 않았습니다.
- 한국어 guard가 경로에 인접한 ASCII code identifier를 외국어 산문으로 계산했습니다.

### 재발 방지

- 로컬 도구 capability 완료 조건은 production TUI에서 `read_file → 서로 다른 로컬
  도구 → 관찰 근거가 포함된 visible answer`를 실제 프로젝트 fixture로 통과하는
  것입니다. Parser와 executor 단위 테스트만으로 capability를 완료 처리하지 않습니다.
- llama.cpp structured schema 변경은 pinned managed parser가 production request를
  HTTP 성공으로 수락하는 candidate job을 필수로 둡니다.
- 파일·코드 도구는 경로·symlink·명령 문법뿐 아니라 대용량 파일, 대용량 디렉터리,
  timeout, cancellation, 출력 누적 상한을 회귀 테스트합니다.
- Runtime route가 없거나 연결되지 않은 결함에 재실행·재설치를 해결책으로 안내하지
  않습니다. 재실행은 이미 반영된 상태·binary를 다시 로드하는 검증 수단일 뿐입니다.
- 한국어 guard는 경로와 snake_case identifier가 함께 있는 한국어 답변을 그대로
  보존하는 회귀 테스트를 유지합니다.
