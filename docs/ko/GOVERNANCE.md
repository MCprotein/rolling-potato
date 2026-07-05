# Governance

`rolling-potato`는 공개 오픈소스 저장소이지만, 현재는 외부 코드 기여와 외부 PR을 받지 않습니다.

## Maintainer 모델

- 현재 maintainer: `MCprotein`
- 제품 방향, 릴리즈, 라이선스, 보안 정책 결정은 maintainer가 관리합니다.
- 외부 maintainer 권한 위임 계획은 아직 없습니다.

## 외부 참여 범위

받을 수 있는 것:

- 버그 리포트
- 사용성 피드백
- 보안 취약점 제보
- 모델 artifact 또는 라이선스 관련 제보

받지 않는 것:

- 외부 코드 PR
- 외부 문서 PR
- 외부 maintainer 신청
- roadmap 결정권 요청

필요한 경우 maintainer가 issue 내용을 바탕으로 직접 변경을 반영합니다.

## 자동 커밋/푸시 운영

이 저장소에서 Codex가 작업할 때는 안전하고 검증 가능한 단위가 끝날 때마다 다음 흐름을 기본값으로 합니다.

1. 변경 범위를 확인한다.
2. 가능한 검증을 실행한다.
3. Conventional Commit 형식으로 커밋한다.
4. `origin main`으로 푸시한다.

사용자가 명시적으로 중단하지 않는 한, 매번 별도 확인 없이 이 흐름을 계속합니다.
