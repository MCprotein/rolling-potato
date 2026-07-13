# 위협 모델

`rolling-potato`는 로컬 코딩 에이전트이므로 일반 CLI보다 더 넓은 위험 표면을 가집니다.

## 보호 대상

- 사용자 source code
- private repository 내용
- credential과 token
- 로컬 파일 시스템
- shell 실행 환경
- 다운로드한 모델 artifact
- operation log
- observability database

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
- 외부 plugin package

## 주요 위협

### 프롬프트 Injection

저장소 파일이나 로그가 모델에게 "정책을 무시하라"고 지시할 수 있습니다.

완화:

- tool policy는 모델 출력이 아니라 runtime이 강제
- 쓰기와 명령 실행은 승인 기반
- final response guard는 별도 단계에서 적용

### 안전하지 않은 Command 실행

모델이 destructive command를 제안할 수 있습니다.

완화:

- command classification
- approval gate
- denylist와 high-risk confirmation
- project boundary check

### 승인 없는 File write

모델이 관련 없는 파일을 수정할 수 있습니다.

완화:

- diff preview
- user approval
- patch apply boundary
- generated/vendor directory 제외

### 모델 Supply chain

잘못된 모델 artifact 또는 변조된 다운로드가 설치될 수 있습니다.

완화:

- manifest schema validation
- SHA-256 verification
- license 확인
- provider trust policy
- checksum 실패 시 등록 금지

### 개인정보 leakage

사용자 코드나 로그가 외부로 나갈 수 있습니다.

완화:

- local backend 기본값
- telemetry 없음
- remote adapter opt-in
- logs redaction
- local user와 visible/normalized model/tool/evidence turn만 영속화하고 전체 backend prompt, hidden response, raw source body는 제외

### 외부 Plugin supply chain

Claude Code/Codex형 plugin이 shell command, MCP server, background process, remote connector, prompt mutation을 포함할 수 있습니다.

완화:

- import 시 직접 실행 금지
- local plugin directory import만 허용
- remote URL, marketplace, registry, catalog, mirror 거부
- path traversal과 boundary-escaping symlink 거부
- source manifest hash 기록
- static capability report 생성
- unsupported capability 명시
- shell/background/MCP는 enable 전 승인 필요
- `bin/`, remote connector, file write path는 기본 차단
- plugin execution은 runtime tool policy와 hook policy를 통과
- import, enable, deny, remove 이벤트 ledger 기록
- marketplace, registry, catalog, mirror source는 지원하지 않음

## MVP 보안 요구

- destructive command policy 위반 0건
- checksum 없는 모델 설치 금지
- 승인 전 파일 쓰기 금지
- final report Korean guard 적용
- credential-like string log redaction
- monitoring store redaction before persistence
- 외부 plugin import는 기본 비활성화 및 inspect/validate 선행

## 미정 사항

- manifest signing 도입 시점
- remote adapter warning UX
- operation log retention 기간
- credential detector 구현 수준
- 외부 plugin을 사용자가 어떻게 획득했는지에 대한 경고 UX
