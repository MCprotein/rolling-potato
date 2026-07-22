# 에이전트 실행 회고

이 문서는 반복 가능한 에이전트 운영 실패와 재발 방지 규칙을 기록합니다. 세션별 작업 일지가 아니라, 다음 작업에서도 적용할 수 있는 교훈만 유지합니다. 강제 규칙은 저장소 루트의 [`AGENTS.md`](../AGENTS.md)가 정본입니다.

## 2026-07-22: release PTY gate가 게시 직후 live update API에 의존함

### 증상

- `v0.46.1` candidate의 전체 test와 Windows native terminal gate는 통과했지만,
  Release 게시 직후 실행된 release test gate에서 Unix `entry_quit`가 10초 timeout됐습니다.
- 첫 PTY test panic이 공유 mutex를 poison해 뒤의 native terminal test 네 개가 실제
  실행 없이 연쇄 실패했고, 플랫폼 asset build는 시작되지 않았습니다.

### 원인

- Native terminal fixture가 setup만 생략하고 startup update check는 그대로 실행해,
  첫 frame과 input read 사이에 실제 GitHub API 호출이 들어갔습니다.
- 게시 직후 아직 aggregate checksum asset이 없는 release를 조회하는 외부 network
  지연이 deterministic PTY input/exit 계약에 섞였습니다.

### 재발 방지

- Native terminal fixture는 `RPOTATO_TEST_SKIP_UPDATE_CHECK=1`을 소유하고 종료 시
  제거하여 live release 상태와 network latency에 의존하지 않습니다.
- Startup update 동작은 별도 adapter/unit 계약으로 검증하고, PTY input/exit 테스트는
  terminal lifecycle만 검증합니다.
- 공유 terminal mutex는 이전 test panic의 poison에서 guard를 회수해 후속 test가 실제
  결과를 보고하도록 하며, 첫 실패를 다수의 가짜 연쇄 실패로 확대하지 않습니다.

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

## 2026-07-21: v0.43.0 binary gate가 candidate 변경을 따라가지 못함

### 증상

- 안내형 TUI header가 바뀌었지만 release-binary smoke는 이전 header를 계속 검사해
  네 Unix 계열 release build가 binary 생성 후 smoke에서 중단됐습니다.
- Windows release job은 ConPTY probe marker 앞의 control sequence와 prompt prefix를
  처리하지 못해 native terminal test에서 중단됐습니다.
- Candidate CI는 release binary를 build만 하고 smoke하지 않았으며 Windows job도
  compile만 수행해 두 결함이 tag 이후에 드러났습니다.

### 원인

- 사용자 surface test와 packaged-binary smoke가 같은 header contract를 공유하지
  않았고 final candidate에 실제 binary smoke가 없었습니다.
- ConPTY probe parser가 marker가 line 처음에 온다고 가정했으며 terminal control
  normalization이 별도 test helper에만 있었습니다.
- Windows runtime test가 release workflow에만 있어 candidate의 exact SHA에서는
  실행되지 않았습니다.

### 재발 방지

- Final candidate CI는 이미 build한 release binary로 guarded install과 interactive
  smoke를 실행합니다.
- Windows candidate job은 compile 뒤 `entry_quit`과 `full_adapter`를 실행해 tag 전에
  native terminal lifecycle을 확인합니다.
- ConPTY control normalization을 공유 helper로 유지하고 prefix/trailer를 포함한
  platform-independent parser 회귀 테스트를 실행합니다.
- 일부만 게시된 source tag는 변경하거나 같은 tag를 재실행하지 않습니다. 실패한
  platform targeted gate를 새 candidate SHA에서 통과시킨 뒤에만 patch tag를 만듭니다.

## 2026-07-19: 병합 완료를 발행 완료로 잘못 전달

### 증상

- v0.42.0 설치 기능 PR이 `main`에 병합됐지만 version branch, tag, GitHub
  Release asset 발행은 진행하지 않은 상태였습니다.
- 설치·clean reinstall은 구현했지만 대칭적인 전체 clean uninstall이 빠진
  상태를 release 준비 완료로 취급했습니다.

### 원인

- `구현 완료`, `PR 병합`, `릴리즈 발행`을 서로 다른 상태로 대조하지 않고
  병합을 사용자 관점의 완료로 축약했습니다.
- 설치 lifecycle을 검토할 때 setup/reset만 확인하고 PATH 해제, binary
  self-delete, 관리형 상태 제거를 포함한 teardown 수용 기준을 확인하지
  않았습니다.

### 재발 방지

- 완료 보고는 `구현`, `기능 PR`, `release PR`, `tag`, `GitHub Release`,
  `asset smoke`, `branch cleanup` 상태를 분리해 사실인 항목만 표시합니다.
- 사용자가 “발행”을 요청한 release 작업은 tag와 GitHub Release URL 및 asset
  검증 전에는 발행 완료로 표현하지 않습니다.
- 설치 기능의 수용 기준에는 install/upgrade/reset뿐 아니라 owned PATH 해제,
  self-delete, managed-state 삭제, user-owned source 보존을 포함한 uninstall
  대칭성을 함께 확인합니다.

## 2026-07-16: v0.34 릴리스 지연

### 증상

- 기능 구현보다 리뷰, 전체 검증, 릴리스 복구에 훨씬 많은 시간이 사용됐습니다.
- 부분 게시된 patch release를 다음 patch tag로 연속 복구하면서 작업 범위가 커졌습니다.
- 중간 상태와 남은 체크리스트가 명확하지 않아 완료 여부를 여러 번 다시 확인했습니다.

### 원인

- Reviewer와 Architect를 중복 실행하고 범위, 시간, token 종료 조건을 두지 않았습니다.
- 기능 수정 중 targeted 테스트와 최종 전체 gate를 분리하지 않아 검증을 반복했습니다.
- CI 전용 native failure의 원인을 충분히 좁히기 전에 release tag를 진행했습니다.
- 장기 진단 명령에 무진행 중단 조건이 없었습니다.
- 완료 판정 전에 최종 candidate SHA 기준 체크리스트를 한 번에 대조하지 않았습니다.

### 재발 방지

- 기능 단위에는 targeted 테스트만 실행하고 즉시 논리 커밋과 푸시로 닫습니다.
- 전체 test, clippy, release build, release gate는 최종 candidate SHA에서 한 번만 실행합니다.
- 독립 리뷰는 범위를 제한한 한 명이 한 번만 수행하며, 15분 또는 80,000 tokens의 기본 한도를 적용합니다.
- 같은 실패의 단순 재시도는 한 번까지만 허용하고 이후에는 원인 진단으로 전환합니다.
- native/platform 실패는 해당 targeted matrix가 통과한 뒤에만 새 release tag를 만듭니다.
- 출력이나 상태 변화가 없는 작업은 10분에 중단하고, 긴 작업의 진행 상태는 60초 이내 간격으로 공유합니다.
- 완료 전에는 PR, SHA, 검증, assets, branch cleanup을 하나의 체크리스트로 확인합니다.

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

## 2026-07-18: package-manager Windows 진단이 전체 qualification을 반복

### 증상

- Scoop과 winget의 Windows lifecycle이 순차적인 초기화, 로컬 source, scope 문제를 하나씩 드러냈습니다.
- Windows 수정 확인 때마다 이미 통과한 Homebrew 네 lane까지 다시 실행돼 불필요한 전체 qualification이 반복됐습니다.
- Scoop manifest 교체 뒤 `git commit` 실패가 확인되지 않아 실제 upgrade 입력이 갱신되지 않은 채 다음 단계가 진행됐습니다.

### 원인

- package-manager workflow에 Windows-only 진단 경로가 없었습니다.
- 로컬 정적 계약은 고정 package-manager client의 Windows 실행 환경과 관리자 scope 차이를 재현하지 못했습니다.
- PowerShell에서 native Git 명령의 nonzero exit를 명시적으로 검사하지 않았고, 파일 교체 후 게시된 version도 확인하지 않았습니다.
- 이전·현재 manifest의 파일 크기와 추출 mtime이 같아 Git stat cache가 덮어쓴 tracked file을 변경되지 않은 것으로 판단할 수 있었습니다.

### 재발 방지

- 개발 중 Windows 실패는 qualification 전용 `lanes=windows`로 먼저 닫고, Homebrew를 포함한 `lanes=all`은 최종 candidate에서 한 번만 실행합니다.
- release와 recovery는 항상 전체 lane을 요구하며 Windows-only 실행으로 cleanup gate를 우회할 수 없게 유지합니다.
- 로컬 bucket 갱신은 파일 교체 후 version을 확인하고 `git add --renormalize`로 tracked manifest를 강제 재색인한 뒤 staged diff와 commit/push/pull의 exit code를 검사합니다.
- winget lifecycle은 runner 권한과 설치 scope를 일치시키고, local manifest 설치는 manifest 기반 uninstall로 정리합니다.

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

## 2026-07-22: 대화형 기본 화면 변경 뒤 release smoke 계약 누락

### 증상

- candidate의 전체 test, lint, release build가 통과한 뒤 binary smoke만 실패했습니다.
- release binary는 새 대화 화면을 정상 출력했지만 smoke가 이전 기본 화면의
  `rpotato | overview` 문자열을 계속 요구했습니다.

### 원인

- 기본 CLI/TUI 화면 계약을 변경하면서 unit·PTY 테스트와 문서는 갱신했지만 release
  binary smoke의 사용자 화면 matcher를 같은 변경 범위로 추적하지 않았습니다.

### 재발 방지

- 기본 CLI/TUI 진입 화면을 변경할 때는 unit, native PTY, release binary smoke의
  사용자-visible marker를 같은 논리 단위에서 함께 갱신합니다.
- debug seam 무시는 특정 과거 화면 하나로 간접 판정하지 않고 현재 기본 화면의 의미
  marker와 debug fault 문구의 부재를 각각 확인합니다.

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

## 2026-07-21: startup update cache를 제품 상태 변경으로 오분류

### 증상

- 새 버전 startup 확인이 `cache/update-latest-v2`를 기록하면서 native terminal의
  무변경 종료 계약이 Linux와 Windows candidate CI에서 실패했습니다.
- 첫 assertion panic 뒤 공유 테스트 mutex가 poison되어 관련 테스트가 연쇄
  실패했고, 실제 결함 범위보다 실패 수가 많아 보였습니다.

### 원인

- 기존 zero-delta 검사는 coordination lock만 예외로 두고, 새로 도입한 bounded
  latest-release cache를 제품 상태와 구분하지 않았습니다.
- startup 기능의 영속 상태 footprint를 추가하면서 기존 process-level 회귀 계약을
  함께 갱신하지 않았습니다.

### 재발 방지

- 무인자 TUI 진입은 workflow·설정·설치 상태를 변경하지 않아야 하지만,
  `cache/update-latest-v2`는 6시간 동안 중복 네트워크 확인을 줄이는 ephemeral
  metadata로 명시적으로 분류합니다.
- zero-delta 예외는 이 정확한 cache 파일과 coordination lock으로 제한하고 Unix와
  Windows 경로 구분자를 모두 회귀 테스트합니다.
- 새 startup side effect를 추가할 때는 native terminal의 entry/quit 계약을 targeted
  검증하며, 새 cache 파일이나 update payload가 암묵적으로 예외가 되게 하지 않습니다.

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

## 2026-07-21: Windows updater가 Get-FileHash cmdlet에 의존함

### 증상

- Windows deferred-update CAS 테스트가 같은 candidate SHA의 targeted workflow에서는
  통과했지만 candidate workflow에서는 exit 1로 실패했습니다.
- 추가한 helper 진단 출력에서 candidate runner의 `powershell.exe`가 `Get-FileHash`
  cmdlet을 찾지 못한 사실을 확인했습니다.

### 원인

- self-update helper의 무결성 확인이 PowerShell 기본 module의 cmdlet 제공과 자동
  로딩을 암묵적으로 가정했습니다.
- 초기 테스트는 parent-process 대기와 CAS 검증까지 결합했고 helper stdout/stderr도
  수집하지 않아 첫 실패에서 실제 원인이 가려졌습니다.
- CAS 동작을 검증하는 테스트가 별도 관심사인 parent-process 종료 관찰에 결합되어
  있었습니다.

### 재발 방지

- self-update helper의 SHA-256 계산은 module cmdlet 대신 PowerShell 5에서도 제공되는
  core .NET `System.Security.Cryptography.SHA256` API를 사용합니다.
- script 계약은 `Get-FileHash` 재도입을 거부하고 .NET SHA-256 사용을 고정합니다.
- 실제 updater는 parent-process 종료 대기를 유지하되, CAS 테스트에는 명시적인 test
  seam을 사용해 대기를 우회하며 helper stdout/stderr를 assertion에 포함합니다.
- Windows 조건부 실행 테스트는 compile 성공과 구분해 exact-HEAD targeted native
  workflow에서 확인한 뒤 새 candidate를 만듭니다.

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

## 2026-07-22: Release matrix의 macOS ARM native test가 무기한 실행됨

### 증상

- `v0.46.2` release matrix에서 macOS 26 ARM의 `entry_quit`는 통과했지만
  `full_adapter`가 21분 넘게 종료되지 않았습니다.
- 같은 SHA와 target의 exact test는 로컬 macOS ARM에서 19.47초에 통과했고,
  실패 job만 새 runner에서 다시 실행했을 때도 정상 통과했습니다.

### 원인

- 첫 GitHub-hosted macOS ARM runner에서 test process가 정체됐습니다.
- Release build job과 native terminal step에 timeout이 없어 테스트 내부의 개별
  10초 대기 제한 밖에서 process가 멈추면 기본 Actions 한도까지 지속될 수 있었습니다.

### 재발 방지

- Release platform build job은 15분, native interactive terminal step은 5분으로
  제한합니다.
- Release workflow contract가 두 timeout을 고정해 guard가 조용히 제거되는 것을
  차단합니다.
- Native test 정체 시 성공한 platform job을 반복하지 않고 exact local test와
  취소된 job log로 환경성 여부를 확인한 뒤 실패 job만 한 번 재실행합니다.
