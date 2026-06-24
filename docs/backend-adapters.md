# Backend Adapters

Backend adapter는 `rpotato`가 추론 backend 차이를 숨기기 위한 경계입니다.

## MVP adapter

MVP는 `llama.cpp` sidecar adapter만 구현합니다.

선택 이유:

- GGUF 지원
- CPU 실행 가능
- macOS와 Windows 우선 지원에 적합
- 작은 모델 runtime과 잘 맞음
- native binding보다 packaging risk가 낮음

## 공통 interface

adapter가 제공해야 할 기능:

- health check
- model metadata 조회
- chat completion
- streaming output
- cancellation
- context length reporting
- backend diagnostics

## `llama.cpp` sidecar

CLI가 관리해야 할 항목:

- backend binary path
- managed backend binary download
- backend archive checksum verification
- backend version detection
- model path
- port selection
- process startup
- health check timeout
- shutdown behavior
- stderr/stdout log capture

실패 처리는 한국어로 좁혀서 보고합니다.

예시:

- backend binary 없음
- model file 없음
- port 사용 중
- checksum 검증 실패 모델
- context length 설정 오류
- backend process crash

사용자가 backend path를 직접 지정한 경우 해당 binary는 사용자 소유입니다. `rpotato uninstall --keep-cache`나 `--purge-cache`는 `rpotato`가 다운로드한 관리형 backend binary만 삭제하고, 사용자 지정 path는 삭제하지 않습니다.

## 후순위 adapter

### LM Studio

장점:

- 이미 설치한 사용자가 많을 수 있음
- demo와 onboarding에 유리함

제약:

- core runtime으로 통합하기에는 외부 앱 의존성이 큼

### Ollama

장점:

- 사용자 기반이 큼
- model management 경험이 단순함

제약:

- 기본 runtime으로는 무겁고 opaque함
- 작은 모델용 tight runtime 정책을 강제하기 어려움

### vLLM / SGLang

장점:

- GPU/server mode에 적합

제약:

- 저사양 로컬 laptop MVP와 맞지 않음
- Windows/macOS CPU 기본 경험과 거리가 있음

## adapter 추가 기준

새 adapter는 다음 조건을 만족해야 합니다.

- 사용자에게 로컬/원격 실행 여부를 명확히 표시한다.
- privacy policy와 command policy를 우회하지 않는다.
- streaming과 cancellation을 지원한다.
- backend diagnostics를 제공한다.
- Korean output guard를 CLI final response 단계에서 그대로 적용한다.
