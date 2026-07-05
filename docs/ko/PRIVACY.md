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

저장하면 안 되는 정보:

- API key
- access token
- password
- private key
- 원문 credential이 포함된 command output
- 사용자 source code 또는 prompt 원문을 기본 monitoring DB에 저장하는 것

## 네트워크 사용

MVP에서 허용되는 네트워크 사용:

- 사용자가 승인한 모델 manifest 조회
- 사용자가 승인한 모델 다운로드
- 릴리즈 업데이트 확인이 추가될 경우 사용자가 끌 수 있어야 함

허용하지 않는 기본 동작:

- 사용자 코드 자동 업로드
- 대화 내용 자동 전송
- command output telemetry
- 외부 LLM API 자동 fallback

## 모니터링

`rolling-potato`는 모델별 token 사용량, latency, backend health, guard result 같은 monitoring metric을 로컬에 저장할 수 있습니다.

기본 원칙:

- monitoring은 local-first입니다.
- 외부 telemetry 전송은 MVP 기본 기능에 포함하지 않습니다.
- raw prompt, source code 원문, credential 포함 command output은 기본 monitoring DB에 저장하지 않습니다.
- export 기능은 사용자 명령으로만 실행합니다.

## 외부 adapter

LM Studio, Ollama, vLLM, SGLang 같은 adapter는 사용자가 명시적으로 설정한 경우에만 사용합니다.

로컬 adapter인지 원격 adapter인지 CLI가 명확히 표시해야 합니다.
