# 에이전트 실행 회고

이 문서는 반복 가능한 에이전트 운영 실패와 재발 방지 규칙을 기록합니다. 세션별 작업 일지가 아니라, 다음 작업에서도 적용할 수 있는 교훈만 유지합니다. 강제 규칙은 저장소 루트의 [`AGENTS.md`](../AGENTS.md)가 정본입니다.

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
