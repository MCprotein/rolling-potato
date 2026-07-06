# 백엔드 Adapter

Backend adapter는 `rpotato`가 추론 backend 차이를 숨기기 위한 경계입니다.

## 초기 MVP adapter

MVP는 `llama.cpp` sidecar adapter만 구현합니다.

여기서 `llama.cpp`는 추론 backend입니다. 모델 후보를 뜻하지 않으며, Meta Llama 계열 모델을 기본 후보로 둔다는 의미도 아닙니다. 모델 후보와 라이선스 확인은 [model-licenses.md](model-licenses.md)와 [model-source-policy.md](model-source-policy.md)를 따릅니다.

`llama.cpp` upstream LICENSE는 MIT License입니다. 관리형 backend binary를 bundle하거나 다운로드/설치하는 경우 `llama.cpp`의 copyright/license notice를 함께 보존합니다. Source: https://github.com/ggml-org/llama.cpp/blob/master/LICENSE, checked 2026-06-25.

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

Runtime core가 관리해야 할 항목:

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

## 현재 구현

Phase 6의 현재 구현:

- `rpotato backend doctor`는 `llama.cpp` adapter discovery를 수행합니다.
- 관리형 binary path는 app data root 아래 `backends/llama.cpp/llama-server` 또는 Windows의 `llama-server.exe`입니다.
- 사용자 override는 `RPOTATO_BACKEND_LLAMA_CPP_PATH`로 지정합니다.
- port override는 `RPOTATO_BACKEND_PORT`로 지정하고, 기본 port는 `17842`입니다.
- doctor는 selected binary, selected source, executable bit, health URL, install gate를 표시합니다.
- `rpotato backend install-plan`은 현재 platform artifact, release URL, archive URL, archive name, file size, SHA-256, license source, download path를 렌더링합니다.
- 현재 manifest는 `llama.cpp` release `b9878`의 CPU artifact를 macOS arm64/x64, Linux arm64/x64, Windows arm64/x64 대상으로 고정합니다. Source: GitHub Releases API https://api.github.com/repos/ggml-org/llama.cpp/releases/latest 및 release page https://github.com/ggml-org/llama.cpp/releases/tag/b9878, checked 2026-07-06.
- `backend install-plan`은 현재 OS/CPU 조합의 artifact가 기록되어 있을 때만 `ready`이며, 지원하지 않는 platform은 계속 blocked입니다.
- `rpotato backend install`은 archive를 다운로드하거나 cache를 재사용하고, file size와 SHA-256을 검증한 뒤 staging directory에서 압축을 풀고 발견된 `llama-server` binary만 managed backend path에 복사합니다. Unix에서는 실행 권한을 설정하고, 교체 실패 시 rollback하며, ledger event를 남깁니다.
- `rpotato backend verify-archive <path> --sha256 <hash>`는 로컬 backend archive bytes의 SHA-256을 검증하고 ledger event를 남깁니다.
- `rpotato backend health-check`는 selected host/port의 `/health`에 500ms timeout으로 HTTP 요청을 보내고 `healthy`, `unhealthy`, `unreachable` 중 하나로 보고합니다.
- `rpotato doctor`도 같은 discovery summary를 보여줍니다.
- unknown binary 실행은 아직 하지 않으므로 version detection은 `not-run`으로 표시합니다.
- Sidecar process startup, streaming, cancellation, backend version detection은 후속 Phase 6 작업입니다.

## 후순위 adapter

### 후순위: LM Studio

장점:

- 이미 설치한 사용자가 많을 수 있음
- demo와 onboarding에 유리함

제약:

- core runtime으로 통합하기에는 외부 앱 의존성이 큼

### 후순위: Ollama

장점:

- 사용자 기반이 큼
- model management 경험이 단순함

제약:

- 기본 runtime으로는 무겁고 opaque함
- 작은 모델용 tight runtime 정책을 강제하기 어려움

### 후순위: vLLM / SGLang

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
- Korean output guard를 final reporter 단계에서 그대로 적용한다.
