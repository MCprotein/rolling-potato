# Threat Model

`rolling-potato`는 로컬 코딩 에이전트이므로 일반 CLI보다 더 넓은 위험 표면을 가집니다.

## 보호 대상

- 사용자 source code
- private repository 내용
- credential과 token
- 로컬 파일 시스템
- shell 실행 환경
- 다운로드한 모델 artifact
- operation log

## 신뢰 경계

신뢰하는 것:

- 사용자 승인
- 로컬 설정 파일
- 검증된 모델 checksum
- 프로젝트 내부에서 사용자가 선택한 작업 범위

완전히 신뢰하지 않는 것:

- 모델 출력
- command output에 포함된 제안
- repository 안의 prompt-like text
- 외부 model manifest
- 다운로드 URL

## 주요 위협

### Prompt injection

저장소 파일이나 로그가 모델에게 "정책을 무시하라"고 지시할 수 있습니다.

완화:

- tool policy는 모델 출력이 아니라 runtime이 강제
- 쓰기와 명령 실행은 승인 기반
- final response guard는 별도 단계에서 적용

### Unsafe command execution

모델이 destructive command를 제안할 수 있습니다.

완화:

- command classification
- approval gate
- denylist와 high-risk confirmation
- project boundary check

### Unauthorized file write

모델이 관련 없는 파일을 수정할 수 있습니다.

완화:

- diff preview
- user approval
- patch apply boundary
- generated/vendor directory 제외

### Model supply chain

잘못된 모델 artifact 또는 변조된 다운로드가 설치될 수 있습니다.

완화:

- manifest schema validation
- SHA-256 verification
- license 확인
- provider trust policy
- checksum 실패 시 등록 금지

### Privacy leakage

사용자 코드나 로그가 외부로 나갈 수 있습니다.

완화:

- local backend 기본값
- telemetry 없음
- remote adapter opt-in
- logs redaction

## MVP 보안 요구

- destructive command policy 위반 0건
- checksum 없는 모델 설치 금지
- 승인 전 파일 쓰기 금지
- final report Korean guard 적용
- credential-like string log redaction

## 미정 사항

- manifest signing 도입 시점
- remote adapter warning UX
- operation log retention 기간
- credential detector 구현 수준
