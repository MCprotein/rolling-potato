# rolling-potato Agent Instructions

이 파일은 `rolling-potato` 저장소 안에서 새 Codex 세션이 열릴 때 따라야 할 프로젝트 로컬 지침입니다.

## 운영 원칙

- 이 프로젝트는 공개 오픈소스 저장소지만, 현재 외부 코드 기여와 외부 PR은 받지 않는다.
- 보안 제보와 사용자 이슈는 받을 수 있지만, 코드 변경 권한과 방향 결정은 maintainer가 가진다.
- 사용자가 별도로 멈추라고 하지 않는 한, 안전하고 되돌릴 수 있는 문서/코드/검증 작업은 계속 진행한다.
- 적당한 작업 단위가 끝나면 직접 검증하고 커밋한 뒤 `origin main`으로 푸시한다.
- 새 세션에서도 사용자가 매번 "커밋해", "푸시해"라고 말하지 않아도 위 커밋/푸시 흐름을 기본값으로 유지한다.
- 실제 출시 작업은 `release/vX.Y.Z` 또는 prerelease에 대응되는 `release/vX.Y.Z-alpha.N` 형태의 version branch에서만 진행한다.
- 출시 branch는 `main`에서 만들고, release checklist 통과 후 `main`에 merge하고, matching tag를 만든 뒤 local/remote branch를 삭제한다.
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
- Rust 코드 변경 후: `cargo fmt`, `cargo test`, 가능하면 `cargo clippy`를 수행한다.
- CLI 동작 변경 후: 관련 `rpotato` 명령 smoke test를 수행한다.
- 검증을 실행할 수 없으면 커밋 메시지나 최종 보고에 이유를 남긴다.
