# 에이전트 실행 회고

이 문서는 반복 가능한 에이전트 운영 실패와 재발 방지 규칙의 색인입니다. 세션별 작업 일지가 아니라, 다음 작업에서도 적용할 수 있는 교훈만 유지합니다. 강제 규칙은 저장소 루트의 [`AGENTS.md`](../AGENTS.md)가 정본입니다.

## 주제별 회고

- [Runtime과 제품 동작](retrospectives/runtime.md): backend, TUI, session, model capability, web grounding 등 실제 제품 동작과 runtime 경계
- [CI와 검증](retrospectives/ci.md): candidate preflight, fixture, PTY 검증, architecture contract, workflow 실행 안정성
- [Release와 배포](retrospectives/release.md): release gate, branch cleanup, binary·package-manager 검증, updater와 게시 상태
- [운영과 거버넌스](retrospectives/governance.md): 리뷰 한도, 외부 게시 승인, 사용자 의도 해석, 제품 surface 안내

새 회고는 가장 가까운 주제 문서에 추가합니다. 여러 주제에 걸치면 최초 실패를 막는 책임 경계를 기준으로 한 곳에만 기록합니다.
