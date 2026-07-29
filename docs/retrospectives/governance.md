# 운영과 거버넌스 회고

리뷰 한도, 외부 게시 승인, 사용자 의도 해석, 제품 surface 안내에서 발생한 반복 가능한 실패를 기록합니다.

## 2026-07-16: 리뷰 token 상한을 프롬프트에만 의존

### 증상

- v0.37.9 단일 리뷰는 약 80초와 제한된 파일 범위 안에서 끝났지만, CLI는 캐시 입력을 포함한 총 입력 token을 190,735로 집계했습니다.
- 리뷰 프롬프트에 60,000 token 한도를 명시했으나 런타임 수준의 강제 종료 조건이 아니어서 저장소의 80,000 token 상한을 보장하지 못했습니다.
- v0.37.10에서는 strict 32,768 context, 24,000 auto-compact, 세 개 명령으로 줄였지만 총 입력 82,925(캐시 입력 54,272)로 상한을 2,925 초과했습니다.

### 원인

- 자연어 지시를 실행기가 강제하는 예산 제한으로 잘못 취급했습니다.
- 리뷰 도중 token 사용량을 관찰하거나 상한에서 중단할 수 있는 실행 표면인지 확인하지 않고 시작했습니다.

### 재발 방지

- 독립 리뷰 시작 전에 사용 중인 실행 표면이 token 사용량을 실행 중 관찰하고 강제로 중단할 수 있는지 확인합니다.
- 강제 token 상한을 지원하지 않으면 검사 파일, 명령별 출력 줄 수, 총 검사 줄 수와 wall-clock을 더 작게 제한하고, token 상한 미보장 사실을 시작 전에 검증 공백으로 기록합니다.
- 완료 후 CLI가 보고한 입력, 캐시 입력, 출력 token을 기록하고 상한 초과 시 두 번째 리뷰를 시작하지 않습니다.
- 다음 CLI 리뷰는 strict context를 최대 24,000, auto-compact를 최대 18,000으로 낮추고 repository read를 하나의 bounded 통합 명령으로 제한합니다. 그 범위로 충분한 결론을 낼 수 없으면 명령이나 리뷰를 추가하지 않고 미확인 위험을 보고합니다.

## 2026-07-18: package-manager 검증을 제3자 게시 승인으로 확대 해석

### 증상

- 생성·검증이 끝난 winget manifest를 Microsoft의 제3자 저장소에 제출하고 public
  fork와 PR을 만들었습니다.
- 대상과 외부 공개 행동을 별도로 설명하지 않은 채 일반적인 계속 진행 지시를 게시
  승인으로 해석했습니다.
- PR 생성으로 자동 validation과 CLA 안내가 시작됐으며, 사용자가 의도하지 않은
  public 기록과 알림이 발생했습니다.

### 원인

- Package-manager roadmap의 최종 상태와 manifest 구현·검증 범위를 실제 외부
  publication과 구분하지 않았습니다.
- 되돌릴 수 있는 first-party 개발 push/PR과 제3자 저장소 기여를 같은 자동 진행
  흐름으로 취급했습니다.
- 외부 쓰기 직전 대상, 공개 내용, 법적 절차, rollback을 제시하고 target-specific
  승인을 받는 gate가 없었습니다.

### 재발 방지

- 일반적인 `진행해`, roadmap 승인, release 요청은 제3자 PR, public repository나
  fork 생성, registry 제출, comment, review, release publication을 승인하지 않습니다.
- Manifest 구현과 lifecycle 검증은 기본적으로 `Generated` 또는 `Validated`에서
  멈추며, `Published`는 대상과 행동을 특정한 별도 승인을 요구합니다.
- 외부 쓰기 전에는 대상, 정확한 변경, 공개 정보, 알림·법적 절차, 정리 방법을
  제시하고 단일 행동 승인을 받습니다.
- CLA, DCO, 이용약관, 라이선스·고용주 권한 진술은 에이전트가 대신 동의하거나
  동의 댓글을 게시하지 않습니다.
- 의도하지 않은 외부 기록이 생기면 추가 댓글을 자동으로 남기지 않고 사실 상태를
  먼저 확인한 뒤 사용자가 명시한 범위만 닫거나 삭제합니다.

## 2026-07-19: package-manager 배포를 프로젝트 의도로 잘못 고정

### 증상

- 사용자가 GitHub Releases에서만 다운로드하도록 의도했지만 별도 Homebrew tap과
  Scoop bucket을 프로젝트 배포 구조로 유지했습니다.
- 메인 저장소의 문서, manifest generator, lifecycle workflow가 외부 저장소
  운영을 전제로 연결돼 저장소와 유지보수 surface가 불필요하게 분산됐습니다.

### 원인

- v0.40.0의 package-manager 실험을 일시적 검증 결과가 아니라 장기 배포 정책으로
  확대 해석했습니다.
- 외부 package channel 채택 여부를 명시적인 제품 결정으로 확인하지 않고 일반적인
  오픈소스 배포 관행을 기본값으로 적용했습니다.

### 재발 방지

- 공식 binary 배포와 다운로드의 정본은 `MCprotein/rolling-potato`의 GitHub
  Releases 하나로 고정합니다.
- Homebrew, Scoop, winget, registry, tap, bucket, 별도 package 저장소는 roadmap,
  release gate, 문서의 설치 경로에 추가하지 않습니다.
- 과거 package-manager 작업은 release history로만 구분하고 generator, fixture,
  workflow를 활성 배포 surface로 복원하지 않습니다.
- 배포 channel을 늘리는 변경은 기능 구현과 별개의 제품·외부 운영 결정으로 보고,
  대상 channel과 저장소를 특정한 사용자 지시 없이는 진행하지 않습니다.

## 2026-07-21: 서브커맨드를 기본 제품 surface로 잘못 안내

### 증상

- 사용자가 `rpotato`만 실행해 Claude Code/Codex형 TUI에 진입하려 했지만 무인자
  실행은 도움말을 출력했습니다.
- 기존 TUI는 예약 명령과 일치하지 않는 일반 텍스트를 agent 요청이 아니라 알 수
  없는 명령으로 처리했습니다.
- 도움말과 사용자 문서가 `rpotato tui`, `backend start --model <path>` 같은 내부·고급
  surface를 기본 사용 흐름처럼 노출했습니다.

### 원인

- 구현된 CLI help를 제품 진입 계약보다 우선해 해석했습니다.
- Claude Code/Codex 대체 경험이라는 상위 목표가 무인자 entrypoint와 일반 텍스트
  composer 회귀 테스트로 고정되지 않았습니다.
- 모델 registry에 기본 모델 경로 해석이 구현돼 있었지만 도움말과 안내가 그 경로를
  반영하지 않았습니다.

### 재발 방지

- attached terminal의 인자 없는 `rpotato`를 기본 TUI entrypoint로 회귀 테스트합니다.
- TUI 예약 명령과 일치하지 않는 일반 텍스트는 runtime의 agent request로 전달하되
  shell command로 직접 실행하지 않습니다.
- `rpotato tui`는 호환 alias, `run`·`backend`·`model` subcommand는 자동화·진단용
  surface로 문서화합니다.
- 첫 실행 backend/model 선택과 설치는 TUI onboarding 안에 두고, GGUF 경로 직접
  입력은 명시적인 고급 override로만 유지합니다.
- 기본 help에는 일상 명령만 노출하고 세부 backend/model/registry 명령은
  `rpotato debug --help` 아래의 호환·진단 surface로 분리합니다.
- Composer 바로 아래의 상태 line에 `model | context | backend | session` 순서를
  고정하고 attached ANSI/redirected plain-text 양쪽을 회귀 테스트합니다.

## 2026-07-23: 한국어 guard와 기본 대화의 제품 의도를 좁게 해석함

### 증상

- 숫자만으로 충분한 계산 답변 `15`가 Hangul이 없다는 이유로 차단됐습니다.
- 정체성 후속 질문이 일반 모델 경로로 들어가 기반 모델의 학습 주체를 rpotato의
  정체성처럼 답했습니다.
- 인사 이외의 일반 지식 질문은 불필요한 repository workflow로 승격됐고, 최신 정보에
  필요한 웹 검색 surface가 없었습니다.
- 첫 final candidate CI에서 구조화된 `proposal id`와 한국어 라벨의 파일 경로를
  외국어 문장으로 오인했고, runtime report의 영문 heading 제거 계약도 달라져 기존
  patch/report 회귀 테스트 3개가 실패했습니다.
- 수정 candidate에서는 `src/lib.rs를`처럼 경로에 한국어 조사가 붙은 정상 답변을
  영문 3단어로 다시 오인했고, 재작성 실패 안내문을 답변으로 채택해 file evidence와
  빈 답변 오류 계약을 훼손했습니다.

### 원인

- 한국어 guard의 성공 조건을 외국어 문장 혼입 방지가 아니라 Hangul 필수 존재로
  구현했습니다.
- 기본 대화 smoke를 인사 한 문장에 한정해 숫자, 정체성 후속 질문, 일반 지식,
  최신 정보 routing을 검증하지 않았습니다.
- 코딩 에이전트의 일반 LLM 기능을 별도 제품 계약으로 고정하지 않았습니다.
- 경로를 `/`와 `.`로 분해한 단어 수에 포함했고, guard의 실패 안내문 자체가 한국어
  검증을 통과한다는 이유로 모델 답변처럼 반환될 수 있었습니다.

### 재발 방지

- 한국어 guard는 중국어·일본어 문자와 독립된 외국어 자연어 문장을 차단하되 숫자,
  수식, 코드, URL, 경로, 기술 용어는 허용하는 회귀 테스트를 유지합니다.
- 기본 대화 candidate는 공백 없는 정체성 질문, 숫자 계산, 일반 지식 질문,
  명시적 웹 검색과 출처 표시를 실제 promoted model TUI smoke에 포함합니다.
- 외국어 혼입은 사실·숫자·코드를 보존하는 한국어 재작성 한 번으로 복구하고,
  재작성 뒤에도 혼입되면 그때만 차단합니다. Guard 실패 안내문이나 projection은
  모델 답변을 대체하지 않습니다.
- Guard 규칙을 바꾸면 guard 단위 테스트뿐 아니라 구조화 patch 성공 보고,
  runtime resume report, workflow runner의 byte-stable projection 회귀 테스트를 함께
  targeted 검증합니다. 알려진 field도 라벨 전체를 건너뛰지 않고 공백 없는 식별자·경로
  또는 값 내부의 외국어 연속 구간으로 구분합니다.
- 경로 판정에는 backtick으로 감싸지 않은 `src/lib.rs를` 형태도 포함하고, read-only
  통합 테스트에서 답변 원문·file evidence·빈 답변 오류 문구를 함께 고정합니다.
- 웹 검색 완료 주장은 고정된 읽기 전용 endpoint의 실제 호출, HTTPS 출처 표시,
  오프라인 override를 함께 검증한 경우에만 사용합니다.

## 2026-07-23: 직접 검색 구현 요청을 hosted REST 연결로 축소 해석

### 증상

- 별도 MCP server와 외부 dependency를 원하지 않는다는 요청을 Brave Search REST
  API를 직접 호출하라는 의미로 잘못 해석했습니다.
- API key가 없으면 `/search`가 동작하지 않아 설치 직후 사용할 수 있다는 제품
  의도와 어긋났습니다.

### 원인

- "직접 구현"을 process/SDK 제거로만 해석하고 hosted 검색 provider와 credential
  의존성까지 제거해야 한다는 경계를 확인하지 않았습니다.
- 경쟁 도구의 공개 구현을 먼저 확인하지 않고 provider 선택부터 했습니다.

### 재발 방지

- 검색 tool의 "직접 구현"은 별도 승인이 없는 한 provider API, API key, MCP
  process, provider SDK가 없는 repository-owned transport/parser/policy를 뜻합니다.
- 경쟁 제품을 참고하라는 요청은 공개 source와 공식 문서로 실제 경계를 먼저
  확인합니다. Hosted tool이나 MCP를 사용한다면 그 사실을 자체 구현으로 표현하지
  않습니다.
- 새 검색 transport는 HTML parser fixture의 실패 회귀 테스트, HTTPS/redirect/query/
  response/source/context 상한, API key 없는 live smoke를 candidate 전에 검증합니다.
- 기능 개발 중 PR은 draft와 label 없는 상태를 유지하고, targeted 검증이 끝난 최종
  HEAD에서만 `release-candidate`를 적용합니다.

## 2026-07-23: 모델 capability와 agent 판단을 고정값·keyword routing으로 덮어씀

### 증상

- Manifest에 더 긴 context가 기록된 모델도 TUI에서 사실상 4096-token 기본값처럼
  취급됐습니다.
- 자연어 웹 요청을 고정 keyword 목록으로 분류해 `/search` 중심 UX가 되었고,
  agent가 질문의 최신성·근거 필요성을 판단하는 경로가 아니었습니다.
- Tool routing model에 local attachment 본문까지 전달하면 model-generated query를
  통해 첨부 내용이 외부 검색 endpoint로 유출될 가능성이 있었습니다.
- Model 전용 projector가 exact cache hit인 경우에도 다시 받을 수 있다는 인상을
  주었고, 준비 실패 시 새 model 선택을 확정하지 않아야 하는 경계가 불명확했습니다.

### 원인

- 평가용 작은-context fixture와 제품 runtime의 model capability를 구분하지
  않았습니다.
- 검색 transport 구현과 agent의 tool-selection policy를 하나의 keyword router로
  묶었습니다.
- Local answer context와 외부 tool decision context를 같은 prompt로 구성했습니다.

### 재발 방지

- 제품 runtime은 source-backed manifest의 model context limit을 사용하고, 작은
  context 값은 benchmark·회귀 fixture에만 명시합니다.
- 자연어 요청의 `WebSearch`·`WebOpen`·`WebFind` 선택은 local model이 담당하고
  `/search`는 명시적 fallback으로만 유지합니다. Offline/no-browse 지시는 runtime이
  agent-selected retrieval을 차단합니다.
- 외부 tool routing model에는 사용자 요청만 제공하며 local attachment 본문은
  검색 이후 local 합성 단계에서만 사용합니다.
- Projector는 model/revision/size/SHA가 일치하는 cache hit를 재사용하고, missing,
  partial, corrupt, revision change일 때만 다시 준비합니다. 준비 실패는 기존
  default model과 ready backend를 변경하지 않습니다.
