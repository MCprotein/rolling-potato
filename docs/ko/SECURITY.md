# 보안 정책

`rolling-potato`는 로컬 파일 읽기, patch 적용, 명령 실행, 모델 다운로드를 다루는 CLI입니다. 따라서 보안 정책은 제품 핵심 기능의 일부입니다.

## 지원 범위

현재는 pre-1.0 단계입니다.

- 지원 버전: `main` branch
- 보안 수정은 우선 `main`에 반영합니다.
- 릴리즈가 생기면 지원 버전 표를 이 문서에 추가합니다.

## 취약점 제보

공개 issue에 credential, private path, private repository 내용, exploit detail을 올리지 마세요.

현재 private security advisory 또는 별도 보안 메일이 설정되지 않았다면, maintainer가 GitHub repository 보안 기능을 먼저 설정해야 합니다. 설정 전에는 민감한 세부 정보를 공개 저장소에 남기지 않는 것을 원칙으로 합니다.

제보에 포함하면 좋은 정보:

- 실행한 `rpotato` 명령
- OS와 architecture
- 관련 설정에서 민감 정보를 제거한 내용
- 재현 단계
- 예상 영향

## 보안 경계

MVP 보안 경계:

- 프로젝트 내부 파일 읽기만 기본 허용
- 파일 쓰기는 diff 표시 후 승인 필요
- side effect가 있는 명령 실행은 승인 필요
- 모델 다운로드는 명시 승인 필요
- 모델 checksum 검증 실패 시 등록 금지
- 로그에 credential 저장 금지
- 검색 결과와 열린 page는 신뢰하지 않는 근거이며 command, file, approval,
  permission 권한을 얻지 못함
- 정적 웹 접근은 URL credential과 local/private/link-local/reserved target,
  허용하지 않는 DNS target을 차단
- v0.50 제한된 browser source candidate는 임시 profile을 시작하고 public
  HTTPS 443 traffic을 address-pinned loopback CONNECT proxy로 강제하며 사용자
  cookie, credential, 인증 session을 재사용하지 않음
- 통합 browser 경로는 익명 search-form 제출만 노출하며 page selector나
  JavaScript를 노출하지 않고 login, 결제, 게시, upload, download, 개인정보,
  project content, attachment 제출을 수행하지 않음
- offline/no-browse를 명시하면 정적·browser retrieval을 모두 비활성화

자세한 위협 모델은 [threat-model.md](threat-model.md), network data 경계는
[PRIVACY.md](PRIVACY.md)를 따릅니다.
