# 개인정보

`rolling-potato`의 기본 방향은 local-first입니다. 사용자 코드, 명령 출력, 대화 내용은 기본적으로 로컬에서 처리되어야 합니다.

## 기본 원칙

- 기본 추론은 로컬 모델과 로컬 backend에서 수행합니다.
- 사용자 프로젝트 파일은 명시된 작업 디렉터리 안에서만 읽습니다.
- telemetry는 MVP 기본 기능에 포함하지 않습니다.
- 모델 가중치는 사용자 승인 후 다운로드합니다.
- 외부 backend adapter를 사용할 경우 사용자가 명시적으로 선택해야 합니다.

## 로컬에 저장될 수 있는 정보

다음 정보는 로컬 설정 또는 로그에 저장될 수 있습니다.

- 설치된 모델 ID
- 모델 파일 경로
- backend 설정
- 작업 승인 기록
- diagnostic 결과
- 오류 로그
- 모델별 token 사용량과 runtime metric
- backend health metric
- app-data attachment directory에 저장하는 사용자 첨부 local file copy

저장하면 안 되는 정보:

- API key
- access token
- password
- private key
- 원문 credential이 포함된 command output
- monitoring만을 위해 전체 backend prompt, hidden reasoning/raw model response, source file 전체 body를 저장하는 것

## 네트워크 사용

MVP에서 허용되는 네트워크 사용:

- 사용자가 승인한 모델 manifest 조회
- 사용자가 승인한 모델 다운로드
- 사용자가 끌 수 있는 선택적 릴리즈 업데이트 확인
- 명시적이거나 최신성이 필요한 읽기 전용 웹 검색. API credential 없이 현재
  질문만 고정 공개 검색 HTML endpoint로 전송합니다.
- 명시적 `WebOpen`. 사용자가 선택한 URL을 직접 요청하며 제한된 normalized
  page는 현재 TUI memory에만 남아 `WebFind`가 검색할 수 있습니다.
- v0.50 source candidate의 명시적 익명 공개 search-form 요청. 새 임시
  Chromium 계열 profile에서 공개 HTTPS page에 제한된 query만 입력하며
  attachment나 project content는 포함하지 않습니다.
- 제한된 browser traffic은 repo-owned loopback HTTPS CONNECT proxy를 반드시
  통과합니다. Proxy는 public IP를 확인·고정하고 port 443만 허용하며 direct
  network fallback이 없습니다. 요청 뒤에는 임시 profile과 process를 정리합니다.

허용하지 않는 기본 동작:

- 사용자 코드 자동 업로드
- 대화 내용 자동 전송
- attachment를 web-search provider로 upload
- project file이나 attachment를 WebOpen target에 자동 upload
- 기존 browser profile, cookie, password, 인증 session 재사용
- browser login, 결제, 게시, upload, download, 개인정보 입력, project·attachment
  제출
- 현재 요청과 무관한 background browser activity
- command output telemetry
- 외부 LLM API 자동 fallback

## 모니터링

`rolling-potato`는 모델별 token 사용량, latency, backend health, guard result 같은 monitoring metric을 로컬에 저장할 수 있습니다.

기본 원칙:

- monitoring은 local-first입니다.
- 외부 telemetry 전송은 MVP 기본 기능에 포함하지 않습니다.
- durable local resume는 user turn과 visible/normalized model/tool/evidence turn을 저장하며 normalized patch action에는 find/replace 또는 verification command 원문 대신 path, action metadata, hash만 저장합니다.
- 전체 backend prompt, hidden/raw model response, source file 전체 body, credential 포함 command output은 transcript storage에서 제외합니다.
- SQLite는 local query용 durable transcript record를 projection할 수 있으며 canonical ledger/artifact state에서 재생성할 수 있습니다.
- export 기능은 사용자 명령으로만 실행합니다.

## 외부 adapter

LM Studio, Ollama, vLLM, SGLang 같은 adapter는 사용자가 명시적으로 설정한 경우에만 사용합니다.

로컬 adapter인지 원격 adapter인지 CLI가 명확히 표시해야 합니다.
