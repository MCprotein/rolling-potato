# CI와 검증 회고

Candidate preflight, fixture, PTY 검증, architecture contract, workflow 실행 안정성에서 발생한 반복 가능한 실패를 기록합니다.

## 2026-07-18: 개발 중 candidate CI를 피드백 루프로 사용

### 증상

- 최근 GitHub Actions 100개 중 16개가 실패했고, 그중 12개가 `refactor-candidate`였습니다.
- Candidate 실패는 clippy 7개, test 4개, fmt 1개로 모두 수정 전 commit에 남았습니다.
- Architecture inventory 누락이 연속 release candidate에서 반복됐고, Windows 조건부 import 오류와 tag/version 불일치는 tag 이후에 발견됐습니다.
- 첫 재발방지 candidate에서는 새 Windows checkout을 추가하면서 action pin 개수 계약을 preflight에 포함하지 않아 tool-pin 검증이 실패했습니다.

### 원인

- `release-candidate` label을 개발 중인 PR에 너무 일찍 적용해 이후 모든 push가 전체 candidate CI를 실행했습니다.
- Targeted 기능 테스트는 실행했지만 최종 후보 전 fmt, clippy, architecture inventory를 묶어 확인하는 preflight가 없었습니다.
- Linux candidate CI가 Windows 조건부 compilation을 확인하지 않아 platform 오류가 release workflow까지 이동했습니다.
- 태그가 아직 없는 pre-tag policy 경로가 잘못된 revision 문자열을 포함했지만 그 fallback을 검증하는 회귀 테스트가 없었습니다.
- Candidate preflight가 workflow의 action pin 개수 계약을 실행하지 않아 CI에서만 drift가 드러났습니다.

### 재발 방지

- 개발 중 PR은 draft로 유지하고, draft PR의 `refactor-candidate` job은 강제로 skip합니다.
- Ready 전 `scripts/ci/verify-pr-candidate-preflight.sh`를 한 번 실행해 action/tool pins, fmt, architecture completeness, warning-deny clippy, workflow contract를 확인합니다.
- Candidate CI는 exact Windows target compile job을 포함하며, 이 job이 통과하기 전에는 merge 또는 tag하지 않습니다.
- Pre-tag policy는 존재하지 않는 tag를 `git rev-parse --verify`로 검사하고 안전하게 current HEAD로 fallback하는 fixture를 유지합니다.
- Action 실패는 단순 재실행하지 않습니다. 기존 실패 유형이면 enforcement gap을 먼저 보강하고, 새로운 유형이면 targeted 회귀 guard와 이 회고를 추가한 뒤 다음 candidate를 만듭니다.

## 2026-07-19: candidate backend fixture의 계산 포트 충돌

### 증상

- 최종 candidate 전체 테스트에서 기능 변경과 무관한 `patch_loop` 한 건이
  fake backend의 `Address already in use`로 실패했습니다.
- fixture가 임시 경로 hash를 30,000~49,999 포트로 변환해, 이미 사용 중인
  포트를 선택해도 복구하지 못했습니다.

### 원인

- 경로별로 달라 보이는 계산 포트를 OS가 할당한 가용 포트로 잘못 취급했습니다.
- backend bind 직전 포트가 충돌할 수 있다는 fixture 복구 경로와 강제 충돌
  회귀 테스트가 없었습니다.

### 재발 방지

- 네트워크 fixture는 고정·hash 포트 대신 OS ephemeral port를 사용합니다.
- bind 시점 충돌은 해당 fixture 안에서만 새 ephemeral port로 제한 횟수만큼
  복구하며, 전체 candidate workflow를 단순 rerun하지 않습니다.
- 포트를 이미 점유한 상태에서 첫 backend start를 강제로 실패시키고 다음
  ephemeral port로 복구되는 targeted 회귀 테스트를 유지합니다.

## 2026-07-21: candidate preflight가 일부 architecture 계약만 실행

### 증상

- 로컬 candidate preflight는 통과했지만 PR CI의 architecture contract 42개 중
  dependency·ownership·line-budget 관련 6개 테스트가 실패했습니다.
- preflight가 migration map 단일 테스트만 실행해, 새 파일 누락 외의 정적 경계
  회귀는 CI에 push한 뒤에야 드러났습니다.

### 원인

- 빠른 preflight를 만들면서 architecture suite 전체가 1초 미만인 정적 테스트라는
  점을 확인하지 않고 가장 익숙한 migration map 검사만 대표로 선택했습니다.
- 하나의 architecture test가 여러 line-budget을 순서대로 검사하므로 첫 assertion을
  고친 뒤 같은 테스트에서 숨은 초과가 이어질 수 있다는 점도 반영하지 않았습니다.

### 재발 방지

- `verify-pr-candidate-preflight.sh`는 migration map 단일 필터 대신
  `architecture_contract` 전체 suite를 실행합니다.
- release workflow contract도 이 전체-suite 명령을 고정해 preflight와 검증 계약이
  서로 다른 명령을 요구하지 않게 합니다.
- architecture CI 실패는 임계값을 올리지 않고 책임 추출 또는 실제 경계 복구로
  닫으며, 변경 뒤 해당 테스트와 전체 architecture suite를 차례로 확인합니다.
- Governed source file을 새로 만들 때는 같은 논리 변경 안에서 migration map의
  file slice와 responsibility inventory를 함께 추가하고, owner line budget도
  targeted architecture test로 확인한 뒤 candidate를 생성합니다.
- 전체 unit test는 PR CI의 정본 검증으로 남기되, 빠른 정적 architecture suite는
  candidate label 전 로컬 preflight에서 실행합니다.

## 2026-07-22: 대화 TUI 검증이 인사 한 문장에 편중

### 증상

- `안녕`은 정상 대화로 처리됐지만 `넌 무슨모델이니`는 일반 coding workflow로
  오분류되어 `backend-call-failed` 내부 보고서가 대화 화면에 노출됐습니다.

### 원인

- conversation-first 수용 테스트를 greeting 한 종류에 집중하고, 사용자가 처음 묻는
  정체성·현재 모델 질문을 핵심 대화 시나리오에 포함하지 않았습니다.
- 현재 모델처럼 runtime이 이미 아는 사실도 생성 모델에 보내는 구조여서 불필요한
  workflow 분류와 backend 실패 가능성을 만들었습니다.

### 재발 방지

- 기본 TUI 변경은 최소한 인사, 에이전트 정체성, 현재 모델, 실제 coding 요청을 서로
  다른 경로로 검증합니다.
- runtime이 확정적으로 보유한 모델·상태 사실은 workflow나 생성 호출 없이 로컬에서
  답하고, 실패 보고서의 workflow id·내부 reason은 대화 화면에 그대로 노출하지
  않습니다.

## 2026-07-22: Windows PTY readiness가 줄 끝 공백에 의존

### 증상

- Windows candidate의 native terminal 테스트가 화면과 composer를 모두 출력하고도
  `› ` 문자열을 찾지 못해 timeout됐습니다.
- ConPTY 캡처에는 프롬프트 glyph `›`가 있었지만 줄 끝 ASCII 공백은 보존되지
  않았습니다.

### 원인

- PTY readiness marker를 보이는 glyph가 아니라 터미널 구현이 생략할 수 있는 trailing
  whitespace까지 포함한 문자열로 고정했습니다.

### 재발 방지

- native terminal readiness는 줄 끝 공백, 색상 escape, cursor 위치처럼 terminal별로
  정규화될 수 있는 바이트에 의존하지 않고 화면에 남는 glyph나 의미 있는 문구로
  확인합니다.
- Windows 조건부 terminal 변경은 추측성 rerun 대신 실패 캡처에서 실제 glyph 출력을
  확인하고 해당 platform test의 matcher를 targeted로 보강합니다.

## 2026-07-21: Rust test selector가 candidate workflow YAML을 무효화

### 증상

- Windows lifecycle test를 한 줄 `run:` scalar로 추가한 뒤 candidate workflow가
  job을 만들지 못하고 workflow load 단계에서 실패했습니다.
- Rust selector의 끝 `tests::` 뒤 공백이 YAML의 `: ` mapping 구문으로 해석됐지만,
  기존 preflight의 workflow contract는 필요한 문자열 존재만 확인해 이를 놓쳤습니다.

### 원인

- `::`로 끝날 수 있는 Rust test selector를 YAML plain scalar에 직접 넣었습니다.
- Candidate workflow 계약에 plain-scalar colon-space 문법을 차단하는 guard가
  없었습니다.

### 재발 방지

- Rust test selector가 들어가는 긴 workflow 명령은 `run: >-` 또는 `run: |` block
  scalar로 작성합니다.
- Release workflow contract는 candidate workflow의 `run: …: ` plain scalar를
  fixture와 함께 거부하며, candidate preflight에서 이 계약을 실행합니다.
- Job이 하나도 생성되지 않은 Actions 실패는 테스트 재실행 대신 workflow 문법부터
  진단합니다.

## 2026-07-22: TUI fixture 통과를 실제 대화 성공으로 오인함

### 증상

- Native terminal fixture와 candidate CI는 통과했지만 설치된 기본 TUI에서 `안녕`도
  `backend-call-failed` 또는 model-action 계약 실패로 끝났습니다.
- 이전 실패 workflow가 active pointer에 남아 이후 입력이 같은 workflow 결과만
  반복해서 표시했습니다.

### 원인

- Terminal fixture는 화면과 입력 계약만 검증했으며 promoted local model을 통과하는
  전체 `request -> backend -> answer-only` 경로를 실행하지 않았습니다.
- Gemma 4 request에는 공식 non-thinking option을 전달하지 않아 제한된 output token을
  reasoning이 모두 소진했습니다.
- 미분류 문장과 project context가 있는 일반 대화를 각각 `small-patch`와
  `inspect-sources`로 승격하고, 부작용 없는 답변에도 model action metadata를
  강제했습니다.

### 재발 방지

- 기본 대화 surface를 변경한 candidate는 promoted model과 backend가 준비된 개발
  환경에서 `rpotato run "안녕"` live smoke를 한 번 실행합니다. 실행할 수 없으면
  release 보고에 validation gap을 명시합니다.
- Live smoke는 exit 0, 새 workflow, `conversation`, `answer-only`, 한국어 final answer를
  함께 확인하며 terminal fixture를 이 증거의 대체물로 사용하지 않습니다.
- 일반 대화의 기본 action은 runtime-owned `answer-only`로 고정하고 명시적인 변경
  signal이 있을 때만 patch workflow로 진입합니다. 한국어 변경 동사는 단위 분류
  테스트뿐 아니라 실제 patch-loop 문장(`고쳐`, `수정`, `바꿔`)으로 회귀 검증합니다.
- 지원 모델의 thinking control은 모델별 공식 근거와 실제 local response를 함께
  검증합니다.
- Terminal workflow pointer 자동 정리는 stop gate가 없는 `failed/cancelled`에만
  적용합니다. `complete`는 성공 증거와 stop gate를 기존 resume 경로에서 다시
  검증한 뒤 정리합니다.
- Pointer 정리 로직을 변경할 때는 실패 workflow의 멱등 복구 테스트와 함께
  `complete` workflow가 검증 전에 지워지지 않는 회귀 테스트를 실행합니다.
- Patch/team 통합 fixture가 patch action을 기대하면 요청 문장에도 명시적인 변경
  의도를 포함합니다. 단순 fixture 이름이나 영어 명사구를 숨은 변경 신호로
  취급하지 않습니다.

## 2026-07-23: 일반 답변과 웹 검색의 실제 surface smoke가 늦었음

### 증상

- TUI 계산은 정상화됐지만 같은 문장을 `rpotato run`으로 실행하면 `설명`이라는
  단어만으로 `explain-error` skill을 선택해 error context 요구조건에서 차단됐습니다.
- `/search`는 검색을 완료하고도 영문 릴리스 제목을 외국어 문장으로 오인해 한국어
  재작성까지 실패하면 검증된 출처를 포함한 결과 전체를 버렸습니다.
- 모델이 만든 `[n]` 표기와 런타임이 수집한 출처 순서가 어긋날 수 있었고, primary
  `--help`에는 이미 지원하는 `/search`가 빠져 있었습니다.

### 원인

- TUI의 conversation 선별과 `rpotato run`의 intent 분류가 서로 다른 회귀 문장
  집합으로 검증됐습니다.
- 4096-token 소형 모델에 최대 14 KiB의 영문 검색 context를 주면서 영문 기술명
  허용 규칙과 출처 렌더링 소유권을 함께 고정하지 않았습니다.
- 단위 테스트 통과 뒤 실제 설치 binary의 계산·검색·help surface를 릴리스 전에
  같은 입력으로 대조하지 않았습니다.

### 재발 방지

- 일반 설명, 계산, 오류 로그 설명 문장은 TUI conversation과 CLI intent 분류 양쪽에
  동일한 회귀 입력을 두고 서로 기대 route가 일치하는지 검증합니다.
- 웹 검색 context와 source 수는 소형 모델 budget으로 제한하고, 요약 생성 실패도
  검증된 HTTPS 출처 표시를 막지 않게 합니다.
- 본문 인용 번호는 모델이 만들지 않으며 출처 URL 목록은 런타임만 렌더링합니다.
- 일반 답변·검색 변경 candidate는 실제 binary에서 계산 한 건, `/search` 한 건,
  primary `--help` 노출을 각각 한 번 smoke한 뒤에만 ready로 전환합니다.

## 2026-07-23: 일반 답변 복구 정책과 Windows fixture 시간 계약이 어긋남

### 증상

- 일반 `backend chat --stream`이 비어 있지 않은 답변을 보존하도록 바뀌었지만,
  patch-loop 통합 테스트 두 건은 여전히 모든 외국어 출력을 exit 3으로 차단하는
  과거 계약을 기대했습니다.
- Windows `full_adapter`는 저장소 seed와 context pack이 계속 진행 중이었지만,
  fixture의 단일 명령 30초 제한에 걸려 종료됐습니다.

### 원인

- 일반 대화의 표시 정책과 patch/실행 결과의 strict 검증 정책을 같은 회귀 테스트로
  취급했습니다.
- GitHub Windows runner에서 실제로 관찰된 단계별 진행 시간보다 fixture 제한이
  짧았고, 플랫폼별 시간 차이를 반영하지 않았습니다.

### 재발 방지

- 일반 대화는 빈 출력만 차단합니다. 외국어 혼입은 재작성과 안전 projection을 한 번
  시도하되, 복구되지 않은 비어 있지 않은 원문도 숨기지 않습니다.
- patch proposal·실행·검증 결과는 별도의 `validate_existing` 경계에서 계속
  fail-closed로 유지하고 외국어 문장 거부 회귀 테스트를 둡니다.
- Native fixture 명령 제한은 Unix 30초, Windows 60초로 유한하게 유지합니다.
  timeout 로그에 ledger와 backend trace를 남겨 실제 무진행과 느린 진행을 구분합니다.
- Full-screen TUI의 PTY raw capture는 동일한 정본 대화를 여러 frame에 다시 그릴 수
  있으므로 marker 개수를 실행 횟수로 해석하지 않습니다. 대신 나타난 모든 exact
  outcome block의 내용과 별도 ledger event delta를 검증합니다.
- 선택형 명령의 native test는 과거의 텍스트 report가 아니라 현재 picker 제목,
  항목, 닫기 동작을 검증합니다.
- Candidate 라벨을 적용하기 전에 remote branch ref와 `refs/pull/<N>/head`가 모두
  candidate SHA와 같은지 확인합니다. Pull ref가 뒤처졌다면 stale run을 취소하고
  새 branch push가 반영될 때까지 잘못된 SHA로 전체 검증을 시작하지 않습니다.

## 2026-07-27: controller 책임 추출 뒤 compile-time 경계 테스트가 이전 파일만 검사함

### 증상

- 요청 실행과 진행 표시를 `controller/request_submission.rs`로 추출한 뒤 targeted
  기능 테스트와 candidate preflight는 통과했지만, PR 전체 test에서 기존
  compile-time 경계 테스트가 `controller.rs` 안의 `runtime.submit_request` 문자열만
  계속 기대해 실패했습니다.
- 실제 runtime 호출 경계와 플랫폼별 native terminal 검증은 정상 동작했습니다.

### 원인

- 모듈 추출 시 production 호출 위치는 갱신했지만, `include_str!`로 구현 파일의
  경계를 검사하는 테스트의 입력 파일 집합을 함께 갱신하지 않았습니다.
- 새 모듈을 architecture inventory에는 등록했으나 unit test의 source-introspection
  의존성을 변경 영향 범위에서 누락했습니다.

### 재발 방지

- 책임을 하위 모듈로 추출할 때는 대상 심볼을 `rg`로 검색해 production call site뿐
  아니라 `include_str!`, source text assertion, architecture inventory를 함께
  갱신합니다.
- Compile-time 경계 테스트는 parent가 하위 모듈을 선언하는지와 실제 authority
  호출이 하위 모듈에 존재하는지를 각각 검증하고, 금지된 직접 의존성은 두 파일
  모두에서 검사합니다.

## 2026-07-29: 플랫폼 helper 이동 뒤 cfg 소유권이 어긋남

### 증상

- macOS targeted test와 candidate preflight는 통과했지만 Windows exact-target
  `cargo check --all-targets --all-features`에서 중복 함수, 숨겨진 공통 codec,
  접근할 수 없는 Windows helper, Unix 전용 테스트 호출이 함께 실패했습니다.

### 원인

- 책임을 하위 파일로 옮길 때 함수 본문만 이동하고 기존 함수·재수출·테스트에 붙은
  `cfg` 조건이 호출자의 플랫폼 범위와 같은지 대조하지 않았습니다.
- 기존 architecture contract는 파일 소유권과 line budget은 검사했지만 공통
  serialization과 플랫폼별 helper의 cfg 대칭성은 고정하지 않았습니다.

### 재발 방지

- 플랫폼 조건부 심볼을 이동할 때 정의, 재수출, 모든 호출자, 관련 테스트의 `cfg`를
  하나의 변경 단위로 검색·대조합니다.
- 공통 codec은 지원 플랫폼 전체에서 컴파일되도록 유지하고, OS별 I/O helper만
  명시적으로 분기합니다.
- System-install과 transition architecture contract는 Windows helper 재수출,
  Unix 전용 테스트, 공통 renderer, Windows no-op 경계의 cfg 계약을 고정합니다.
- macOS에서 Windows C toolchain 부재로 cross-check가 Rust 소스까지 도달하지 못하면
  이를 성공으로 간주하지 않고 Windows exact-target CI를 최종 증거로 사용합니다.

## 2026-07-27: PTY 화면 테스트가 production 의미 분류기와 다른 입력을 사용함

### 증상

- Agent 정체성 shortcut을 정확 일치로 좁힌 뒤 PR 전체 test에서 스크롤·composer
  보존 테스트가 실패했습니다.
- 테스트는 화면 행을 채우기 위해 `넌 누구야 1`처럼 suffix를 붙였지만, 이 입력은
  더 이상 정체성 shortcut이 아니어서 모델 미선택 오류 경로로 진입했습니다.

### 원인

- 화면 동작을 검증하는 PTY fixture가 의미 분류기의 암묵적 허용 범위에 의존했고,
  정체성 분류기의 계약 테스트와 화면 테스트의 책임이 분리되지 않았습니다.

### 재발 방지

- PTY UX 테스트는 production 분류기에서 유효한 실제 명령을 그대로 반복해 화면
  상태를 구성합니다. 행 수를 만들기 위한 임의 suffix로 의미를 바꾸지 않습니다.
- 의미 분류 경계는 전용 unit test가 담당하고, scroll·draft 같은 화면 테스트는
  분류 결과가 안정적인 입력만 사용합니다.

## 2026-07-28: PR 상태 이벤트가 검증 중인 candidate run을 취소함

### 증상

- Draft PR에 `release-candidate` label을 붙인 뒤 ready로 전환하자, 먼저 시작한
  candidate run이 뒤늦게 전달된 다른 PR action run에 의해 취소됐습니다.
- 후속 run은 event snapshot의 draft·label 조건 때문에 skip되어 정확한 candidate
  SHA에 대한 GitHub 검증이 남지 않았습니다.

### 원인

- Candidate workflow concurrency group이 PR 번호만 사용해 `labeled`,
  `ready_for_review`, `synchronize`처럼 의미가 다른 action을 모두 같은 실행으로
  취급했습니다.
- Job 조건에서 skip될 event도 workflow concurrency에는 먼저 참여하므로, 실행할
  작업이 없는 event가 정상 candidate run을 취소할 수 있었습니다.

### 재발 방지

- Candidate concurrency group에 `github.event.action`을 포함해 같은 action의
  오래된 run만 취소하고, label·ready 전환처럼 다른 action끼리는 취소하지 않게
  합니다.
- Release workflow contract가 action-scoped concurrency와
  `cancel-in-progress: true`를 함께 고정해, 빠른 재푸시 정리는 유지하면서 상태
  전환 간 교차 취소는 다시 들어오지 못하게 합니다.

## 2026-07-31: 필수 backend endpoint 추가가 통합 fixture에 전파되지 않음

### 증상

- 정확한 입력 token 계산을 위해 `/v1/chat/completions/input_tokens` 호출을 필수
  단계로 추가한 뒤 unit test는 통과했지만 PR의 `patch_loop` 29건이 함께 실패했습니다.
- 통합 fake sidecar가 새 endpoint에도 기존 generation 응답을 반환해 strict parser가
  `choices` 필드를 거부했습니다.
- Preflight가 generation admission보다 먼저 실행되어 동시 요청이 중복 preflight를
  수행할 수 있었고, stalled preflight는 기존 cancel lifecycle에서 보이지 않았습니다.
- Endpoint 응답을 분리한 뒤에도 공용 fixture의 generation request marker가 token
  preflight까지 세어, subagent 사용자 여정의 실제 생성 3회가 6회로 집계됐습니다.

### 원인

- 새 transport phase를 adapter 내부 계산 변경으로만 취급하고, endpoint를 모사하는
  모든 fake sidecar와 사용자 journey fixture를 변경 영향 범위로 관리하지 않았습니다.
- 단위·mock 테스트가 요청 body와 parser를 각각 검증했지만
  `endpoint → response contract → admission/cancel/timeout`의 결합 계약은 검증하지
  않았습니다.
- 필수 preflight와 generation을 하나의 operation lifecycle로 먼저 모델링하지 않고
  호출 순서에 기능을 덧붙였습니다.

### 재발 방지

- 필수 backend endpoint나 transport phase를 추가·변경할 때는 `rg`로 production
  caller, fake sidecar, request accounting, timeout·cancel·single-flight fixture를
  함께 찾아 같은 논리 변경에서 갱신합니다.
- Fake sidecar는 endpoint별 request와 response schema를 엄격히 분리하고, 잘못된
  generation 응답이 token preflight에서 성공으로 해석되지 않게 합니다.
- 공용 fake sidecar의 request marker는 `input_tokens`와 `generation`을 구분하고
  size budget은 두 transport phase를 모두 집계합니다. 모델 결정 검사용 body
  marker는 generation만 기록하며, subagent lifecycle exact test를 candidate
  preflight에 포함해 request accounting 의미가 다시 합쳐지지 않게 합니다.
- 필수 preflight는 별도 요청이 아니라 generation operation의 phase로 취급합니다.
  Admission과 cancel visibility를 먼저 만들고, 하나의 absolute deadline에서 각
  transport 직전 남은 시간을 계산합니다.
- Endpoint 계약을 바꾼 candidate는 관련 unit test 뒤 해당 user-journey integration
  suite를 로컬에서 한 번 실행하고, 같은 suite 실패를 CI 단순 재실행으로 넘기지
  않습니다.

## 2026-08-01: 책임 추출 뒤 migration map과 source contract가 함께 갱신되지 않음

### 증상

- v0.55 candidate preflight의 전체 architecture contract에서 새 책임 소유자 21개가
  migration map에 누락됐고, TUI runtime port 검사는 이동 전 facade만 읽었습니다.
- projector 검증 helper와 회귀 테스트가 기존 backend·records 파일의 line budget을
  넘겨 책임 경계 테스트가 실패했습니다.

### 원인

- 기능 단위 targeted contract만 실행해 새 파일 자체의 경계는 확인했지만, 저장소의
  모든 governed file을 대조하는 전체 architecture inventory 검증을 candidate 직전까지
  실행하지 않았습니다.
- 구현 이동, source-text contract, migration map, regression owner 분리를 하나의
  architecture 변경 단위로 닫지 않았습니다.

### 재발 방지

- 책임 추출이나 새 governed file 추가 시 production owner, source-text assertion,
  `docs/architecture-migration-map.json`의 file record와 responsibility inventory를 같은
  커밋에서 갱신합니다.
- 기능 테스트가 커진 경우 line budget을 올리지 않고 별도 책임 소유자로 먼저 분리하며,
  변경된 owner의 exact contract와 전체 migration-map contract를 커밋 전에 실행합니다.
- Candidate preflight 실패는 실패한 contract만 재현해 수정하고, 새 HEAD에서만 최종
  preflight를 다시 실행합니다. 같은 HEAD의 전체 preflight는 반복하지 않습니다.

## 2026-08-03: 합성 입력 테스트가 실제 터미널 붙여넣기 계약을 대체함

### 증상

- Controller에 경로 문자열을 직접 넣는 테스트는 통과했지만 macOS 이미지 붙여넣기는
  임시 PNG 경로가 slash command로 분류되어 실패했습니다.
- TUI가 mouse-reporting mode를 기본 활성화해 터미널의 일반 드래그 선택도 가로챘습니다.
- `Ctrl+V`는 raw mode에서 `IEXTEN`이 남아 애플리케이션 이벤트가 아니라 POSIX
  literal-next 입력으로 처리됐습니다.

### 원인

- 테스트가 terminal escape mode, bracketed-paste provenance, OS clipboard bridge를
  건너뛰고 controller 이후 상태만 주입했습니다.
- mouse-wheel 지원과 사용자의 native selection을 별도 기능으로 모델링하지 않았습니다.

### 재발 방지

- 터미널 입력 변경은 controller unit test에 더해 실제 PTY에서 bracketed paste mode가
  켜진 뒤 정확한 macOS 임시 경로와 `Ctrl+V` byte가 application action에 도달하는지
  검증합니다.
- 기본 TUI는 mouse-reporting mode를 켜지 않습니다. 별도 선택 UI가 없는 한 drag
  selection과 terminal-native scrollback을 보존합니다.
- 이미지 붙여넣기는 slash command dispatch보다 먼저 분류하고, 비어 있는 bracketed
  paste와 명시적 clipboard shortcut은 OS clipboard adapter를 거치며 입력 draft를
  보존합니다.

## 2026-08-03: 단발성 fake sidecar가 반복형 agent loop를 검증하지 못함

### 증상

- 단일 structured response fixture는 첫 도구 선택만 검증해, observation 뒤 모델이
  다음 도구 또는 답변을 선택하는 실제 반복 경로가 없어도 테스트가 통과했습니다.
- 다중 turn fixture를 추가하자 `Content-Length`가 아닌 첫 colon header를 길이로
  오인해 큰 observation request를 빈 body로 처리했습니다.
- 병렬 web adapter 테스트는 공유 worker pool의 전역 제출 수를 exact delta로 비교해
  다른 테스트의 정상 작업에도 간헐적으로 실패했습니다.

### 원인

- Fake sidecar의 응답 모델이 one request/one response에 고정되어 production의
  model→tool→observation lifecycle을 표현하지 못했습니다.
- HTTP header parser와 worker 상한 검증이 각각 실제 header 이름과 test-local 완료
  신호가 아니라 우연한 순서와 공유 전역 counter에 의존했습니다.

### 재발 방지

- 반복형 agent 기능은 순서가 있는 structured response fixture로 production terminal
  경계를 통과하며 model request 수, observation envelope과 현재 typed tool activity를
  함께 검증합니다.
- 하나의 논리 tool route가 다른 tool을 내부 실행하지 못하도록 Search 결과 뒤 model
  request를 먼저 관찰하고, Search→Open→Find→Answer 각 단계 사이에 직전 observation만
  전달되는 production PTY 회귀를 유지합니다.
- 반복 호출 상한은 직전 호출이 아니라 현재 turn의 전체 `(tool, normalized input)`
  이력을 기준으로 하며, 시간 상한은 매 admission 시 turn 시작 시각에서 다시 계산합니다.
- Fake HTTP parser는 header 이름을 case-insensitive exact match한 뒤 body 길이를 읽고,
  여러 header와 4 KiB를 넘는 request body 회귀를 유지합니다.
- 공유 worker pool 테스트는 전역 exact delta 대신 해당 test가 소유한 completion
  signal과 deadline을 검증합니다.

## 2026-08-03: 후속 모델 판단 조건이 직접 웹 도구 실행을 차단함

### 증상

- Agent loop 도입 뒤 모델이 아직 선택되지 않은 TUI에서 `/open`, `/search`, `/find`가
  웹 fixture를 읽기 전에 context length 오류로 중단됐습니다.
- Targeted agent-loop 테스트는 context를 항상 제공해 이 base capability 회귀를
  발견하지 못했고, candidate 전체 테스트에서만 기존 conversation journey 두 건이
  실패했습니다.

### 원인

- 웹 관찰 뒤의 선택적 모델 합성에 필요한 context length를 읽기 전용 웹 도구 자체의
  필수 입력으로 올렸습니다.
- 기존 no-model conversation journey가 실제 화면을 assertion 실패에 포함하지 않아
  CI 로그만으로 선행 차단 지점을 바로 확인하기 어려웠습니다.

### 재발 방지

- 직접 지정한 읽기 전용 웹 도구는 모델 readiness와 분리합니다. Context가 있으면
  후속 모델 판단을 수행하고, 없으면 런타임이 검증한 출처·페이지·find fallback으로
  완료합니다.
- Web tool context는 모델 합성 능력을 `Option`으로 표현하며 임의 기본 token 수를
  넣지 않습니다.
- No-model `/open → /find`와 `/search → source 선택 → /find` journey를 유지하고,
  핵심 화면 assertion에는 렌더 결과를 실패 메시지로 포함합니다.
