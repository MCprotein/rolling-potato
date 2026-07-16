# rolling-potato Agent Instructions

이 파일은 `rolling-potato` 저장소 안에서 새 Codex 세션이 열릴 때 따라야 할 프로젝트 로컬 지침입니다.

## 운영 원칙

- 이 프로젝트는 공개 오픈소스 저장소지만, 현재 외부 코드 기여와 외부 PR은 받지 않는다.
- 보안 제보와 사용자 이슈는 받을 수 있지만, 코드 변경 권한과 방향 결정은 maintainer가 가진다.
- 사용자가 별도로 멈추라고 하지 않는 한, 안전하고 되돌릴 수 있는 문서/코드/검증 작업은 계속 진행한다.
- 적당한 작업 단위가 끝나면 관련 범위만 검증하고 논리 단위로 커밋한 뒤 현재 feature/release branch로 푸시한다.
- 새 세션에서도 사용자가 매번 "커밋해", "푸시해"라고 말하지 않아도 위 커밋/푸시 흐름을 기본값으로 유지한다.
- 작업 branch는 PR로 `main`에 반영하고 기본 병합 방식은 squash merge로 한다.
- 실제 출시 작업은 `release/vX.Y.Z` 또는 prerelease에 대응되는 `release/vX.Y.Z-alpha.N` 형태의 version branch에서만 진행한다.
- 출시 branch는 `main`에서 만들고, release checklist 통과 후 `main`에 merge하고, matching tag를 만든 뒤 release workflow가 remote branch를 삭제하게 하고 local branch도 삭제한다.
- 출시 전에는 `scripts/release/verify-release-policy.sh`를 실행해 branch/tag/version 정책을 확인한다.
- destructive command, credential 필요 작업, 외부 production 변경, 라이선스 변경처럼 되돌리기 어렵거나 권한이 필요한 작업은 사용자 확인이 필요하다.

## 커밋 규칙

커밋 메시지는 Conventional Commits 형식을 사용한다.

```text
type(scope): title
```

예시:

```text
docs(governance): add open source operating policy
feat(cli): scaffold rpotato command router
fix(model): reject invalid manifest checksum
test(guard): cover Korean output leakage
```

사용 가능한 type 예시:

- `feat`
- `fix`
- `docs`
- `test`
- `refactor`
- `chore`
- `ci`
- `build`

## 저장소 위생

- `.omx/`, 로컬 모델 파일, 빌드 산출물, 임시 로그는 커밋하지 않는다.
- 모델 가중치는 저장소에 포함하지 않는다.
- 모델 URL, checksum, license는 manifest와 문서에서 추적한다.
- 모델 이름, 라이선스, 성능, RAM 요구량, backend 호환성, multimodal 지원 여부 같은 모델 관련 claim은 출처 없이 확정하지 않는다. 출처가 없으면 `미확정`으로 표시하고 [docs/model-source-policy.md](docs/model-source-policy.md)를 따른다.
- 사용자에게 보이는 문서와 최종 보고는 한국어를 기본으로 한다.

## 기본 검증

- 문서 변경: 링크, 오타, 미완성 마커 스캔을 수행한다.
- 기능 개발 중에는 변경 모듈의 unit/targeted 테스트만 실행한다. 작은 수정마다 전체 테스트, 전체 release gate, 전체 리뷰를 반복하지 않는다.
- 기능 범위가 안정되면 적합한 Reviewer 또는 Architect 한 명으로 독립 리뷰를 정확히 한 번 실행한다. 두 역할을 중복 실행하거나 수정 후 전체 리뷰를 다시 돌리지 않고, 발견된 결함은 targeted 회귀 테스트로 닫는다.
- 전체 `cargo test --locked`, 전체 clippy, release build와 release gate의 정본 검증은 PR CI가 최종 candidate `HEAD`에 대해 수행한다. 같은 `HEAD`에서 통과한 전체 검증을 로컬에서 중복 실행하지 않는다.
- CI 실패를 수정할 때는 실패 범위의 targeted 검증만 로컬에서 실행하고 푸시한다. PR `HEAD`가 바뀌었으므로 CI가 새 커밋을 다시 검증하는 것은 허용한다.
- 태그 이후 플랫폼별 빌드, 패키징, checksum, release asset smoke 검증은 배포 검증으로 취급하며 개발 중 전체 테스트 반복과 구분한다.
- CLI 동작 변경 후: 관련 `rpotato` 명령 smoke test를 수행한다.
- 검증을 실행할 수 없으면 커밋 메시지나 최종 보고에 이유를 남긴다.

## 실행 시간과 반복 제한

- 작업 시작 시 성공 조건, 변경 범위, 필요한 targeted 검증, 최종 전체 검증, 중단 조건을 짧게 확정한다.
- 각 논리 단위는 `구현 -> targeted 검증 -> 커밋 -> 푸시`로 닫는다. 여러 기능을 하나의 거대한 미커밋 변경으로 누적하지 않는다.
- 동일한 가설과 동일한 입력으로 실패한 명령을 두 번 넘게 반복하지 않는다. 두 번째 실패부터는 재시도 대신 원인 진단이나 접근 변경으로 전환한다.
- 장기 명령과 하위 에이전트에는 범위와 종료 조건을 명시한다. 출력이나 상태 변화 없이 10분이 지나면 중단하고 원인을 다시 평가한다. 진행 중인 정상 빌드처럼 계속 실행할 근거가 있으면 그 근거와 상태를 사용자에게 보고한다.
- 사용자에게 진행 중인 작업을 60초 넘게 무응답 상태로 두지 않는다. 긴 명령은 짧게 yield/poll하고, 완료된 증거와 다음 중단 조건을 간결하게 알린다.
- 독립 리뷰는 변경 파일과 수용 기준으로 범위를 제한한 한 명의 Reviewer 또는 Architect가 한 번만 수행한다. 기본 한도는 15분 또는 80,000 tokens 중 먼저 도달하는 시점이며, 한도 이후에는 두 번째 전체 리뷰를 시작하지 않고 미확인 위험을 보고한다.
- 최종 전체 검증을 시작할 때 candidate commit SHA를 기록한다. 같은 SHA에서 통과한 전체 test, clippy, build, release gate를 다시 실행하지 않는다.
- 플랫폼 CI 실패는 해당 플랫폼과 실패 테스트의 targeted workflow가 통과하기 전까지 새 patch tag로 우회하지 않는다. 복구 릴리스를 추측성으로 연속 발행하지 않는다.
- `완료` 보고 전에 커밋/푸시, PR 상태, candidate SHA, 필수 검증, release asset, 원격·로컬 branch 정리 여부를 체크리스트로 한 번 대조한다. 하나라도 미확인이라면 완료로 표시하지 않는다.

## 지속 개선

- 예상 밖의 30분 이상 지연, 동일 실패의 반복, 잘못된 완료 보고, 사용자의 운영 방식 교정이 발생하면 [에이전트 실행 회고](docs/agent-retrospectives.md)에 원인과 재발 방지 규칙을 기록한다.
- 회고에는 재사용 가능한 사실만 남기고 raw prompt, secret, 개인 정보, 임시 로그는 기록하지 않는다.
- 다음 작업이 기존 사고와 같은 검증, 리뷰, CI, release 영역을 건드리면 관련 회고를 먼저 확인한다.
- 반복해서 유효성이 확인된 재발 방지 규칙은 이 `AGENTS.md`로 승격한다. 안전·권한 경계를 약화하는 방향으로 규칙을 자동 변경하지 않는다.
- 운영 규칙 변경도 일반 코드와 동일하게 작은 diff, 문서 검증, 논리 커밋, PR 기록을 남긴다.
