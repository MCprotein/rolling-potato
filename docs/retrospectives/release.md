# Release와 배포 회고

Release gate, branch cleanup, binary·package-manager 검증, updater와 게시 상태에서 발생한 반복 가능한 실패를 기록합니다.

## 2026-07-26: macOS release-only PTY timeout이 blocking Drop 뒤에 원인을 숨김

### 증상

- v0.49.3 release workflow에서 Linux와 Windows는 통과했지만 macOS ARM·Intel의
  `native_terminal::full_adapter`가 모두 외부 5분 timeout으로 종료됐습니다.
- Test process가 종료되지 않아 실제 assertion이나 마지막 진행 stage가 log에
  남지 않았고, aggregate checksum과 최종 asset 검증도 실행되지 않았습니다.
- PR candidate에는 macOS native-terminal exact job이 없어 tag 게시 전 같은
  platform 경로를 검증하지 못했습니다.

### 원인

- Unix `NativePty::drop`이 `SIGTERM`을 보낸 뒤 blocking `waitpid`를 호출했습니다.
  Child가 신호를 처리하지 못하는 상태이면 assertion unwind 중인 test process도
  함께 무기한 대기했습니다.
- Stage trace가 Windows 경로에만 있었고 release test가 stderr를 capture하여 macOS
  실패 위치를 workflow timeout 전에 확인할 수 없었습니다.
- 전체 test 통과를 platform-native PTY 동작까지 증명하는 것으로 잘못 간주해,
  release에서 처음 실행하는 macOS exact scenario가 남았습니다.

### 재발 방지

- Unix PTY cleanup은 짧은 `SIGTERM` grace period 뒤 `SIGKILL`로 escalation하고,
  child를 suspend한 exact test로 종료 시간 상한을 검증합니다.
- Native-terminal candidate·release test는 모든 platform에서 stage trace를
  `--nocapture`로 즉시 출력합니다.
- macOS native-terminal exact job을 PR candidate gate에 두어 merge와 tag 전에
  동일한 runner·target 경로를 통과해야 합니다.
- Platform release 실패는 같은 workflow를 단순 재실행하지 않습니다. 원인 guard와
  targeted candidate 검증을 추가한 recovery patch만 새 tag로 진행합니다.

## 2026-07-24: release PR merge가 cleanup 전에 branch를 삭제함

### 증상

- `v0.48.1`의 release test, 5개 platform build, packaged smoke, 개별·통합 checksum과
  published asset 검증은 모두 통과했지만 마지막 release branch cleanup job만
  실패했습니다.
- Cleanup policy는 검증할 `release/v0.48.1` remote branch가 이미 없다고 보고
  종료했습니다.

### 원인

- Release PR을 squash merge하면서 일반 feature PR과 같은 `--delete-branch` option을
  사용해 asset 검증이 끝나기 전에 remote release branch를 삭제했습니다.
- Release workflow는 matching branch의 candidate가 tag commit에 merge됐거나
  tree-equivalent인지 확인한 뒤 branch를 삭제하도록 설계되어 있어, 조기 삭제를
  안전하지 않은 lifecycle 위반으로 차단했습니다.

### 재발 방지

- Release PR merge에는 `--delete-branch`를 사용하지 않고 `release-binaries` cleanup
  job만 remote release branch를 삭제합니다.
- 조기 삭제가 발생했다면 실패 job을 그대로 재실행하지 않습니다. Exact release
  candidate SHA로 matching branch를 복원하고 tag/main/tree 관계를 다시 검증한 뒤
  cleanup job만 재실행합니다.
- 일반 feature PR의 branch 삭제와 release branch cleanup을 서로 다른 운영
  lifecycle로 취급합니다.

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
