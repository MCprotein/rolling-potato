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
- Agent가 선택한 웹 검색은 현재 사용자 요청의 제한된 literal projection만
  고정된 공개 HTML 검색 endpoint로 HTTPS 전송합니다. 대화 이력에만 있던 값은
  거부하며 routing model에는 local attachment 본문을 전달하지 않고 API
  credential도 사용하지 않습니다. Offline/no-browse 지시는 retrieval을
  비활성화합니다.
- `WebOpen`은 HTTP 입력을 HTTPS로 승격하고 URL credential, local/private/
  link-local/reserved target과 DNS 응답을 실제 direct 연결에 사용하는 resolver에서
  차단하며 proxy routing을 비활성화합니다. 제한된 동일 host redirect만 추적하고
  다른 host redirect는 새로운 명시적 open을 요구합니다.
- 제한된 검색 highlight와 열린 페이지 text는 신뢰하지 않으며 command, file,
  permission 권한을 얻지 못합니다. 열린 content는 `WebFind`를 위해 현재 TUI
  memory에만 유지하고 durable transcript source로 저장하지 않습니다.
- 붙여넣은 첨부는 허용 형식과 크기 상한을 만족하는 regular non-symlink 파일만 local
  app data에 복사하며 web-search provider로 보내지 않습니다. PNG/JPEG bytes는
  dispatch 시 signature와 hash를 다시 검증하고, image inference에는 정확히 검증된
  model/projector 조합이 필요합니다.
- logs redaction
- local user와 visible/normalized model/tool/evidence turn만 영속화하고 전체 backend prompt, hidden response, raw source body는 제외

### 제한된 브라우저 오용

공개 page가 SSRF, DNS rebinding, prompt injection, 인증된 side effect, 의도하지
않은 file transfer, process/profile leak를 시도할 수 있습니다.

완화:

- model은 typed `search-form` operation만 요청할 수 있고 deterministic fallback은
  명시적인 공개 검색 site 지시에만 적용합니다.
- 매 실행은 새 임시 profile과 process group을 사용합니다. 통합 경로는 사용자의
  browser profile, cookie, password, login session을 불러오지 않습니다.
- 모든 page traffic은 repo-owned loopback HTTPS CONNECT proxy를 반드시
  통과합니다. Port 443만 허용하고 DNS를 public address인지 확인한 뒤 해당
  address를 연결에 고정합니다. QUIC과 non-proxied WebRTC UDP를 비활성화하고
  direct fallback을 두지 않습니다.
- runtime은 제한된 accessibility tree를 관찰하고 opaque handle로만 action을
  수행합니다. 사이트별 selector와 `Runtime.evaluate` JavaScript를 노출하지
  않습니다.
- application coordinator는 제한된 검색어만 입력·제출하고 검증한 공개 최종
  URL과 제한된 text를 읽습니다. Login, 결제, 게시, upload, download,
  개인정보, project content, attachment 제출 경로를 노출하지 않습니다.
- navigation 시도, 읽기 시간, byte, element count를 제한하며 성공, timeout,
  error에서 process group과 임시 profile을 정리합니다.
- page content는 신뢰하지 않는 evidence이며 command, file, approval,
  permission 권한을 넓힐 수 없습니다.
- offline/no-browse를 명시하면 제한된 browser 경로를 비활성화합니다.

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
