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

사용자가 backend path를 직접 지정한 경우 해당 binary는 사용자 소유입니다. `rpotato uninstall --keep-cache`나 `--purge-cache`는 `rpotato`가 다운로드한 관리형 backend payload만 삭제하고, 사용자 지정 path는 삭제하지 않습니다.

## 현재 구현

Phase 6의 현재 구현:

- `rpotato backend doctor`는 `llama.cpp` adapter discovery를 수행합니다.
- 관리형 binary path는 app data root 아래 `backends/llama.cpp/llama-server` 또는 Windows의 `llama-server.exe`입니다.
- 사용자 override는 `RPOTATO_BACKEND_LLAMA_CPP_PATH`로 지정합니다.
- port override는 `RPOTATO_BACKEND_PORT`로 지정하고, 기본 port는 `17842`입니다.
- doctor는 selected binary, selected source, executable bit, health URL, install gate를 표시합니다.
- `rpotato backend install-plan`은 현재 platform artifact, release URL, archive URL, archive name, file size, SHA-256, license source, download path를 렌더링합니다.
- 현재 manifest는 `llama.cpp` release `b9982`의 CPU artifact를 macOS arm64/x64, Linux arm64/x64, Windows arm64/x64 대상으로 고정합니다. Artifact name, byte size, SHA-256 digest는 2026-07-13 확인한 GitHub Releases API https://api.github.com/repos/ggml-org/llama.cpp/releases/latest 및 release page https://github.com/ggml-org/llama.cpp/releases/tag/b9982 를 근거로 합니다.
- `backend install-plan`은 현재 OS/CPU 조합의 artifact가 기록되어 있을 때만 `ready`이며, 지원하지 않는 platform은 계속 blocked입니다.
- `rpotato backend install`은 archive를 다운로드하거나 cache를 재사용하고, file size와 SHA-256을 검증한 뒤 staging directory에서 압축을 풀어 release payload를 managed backend directory에 배치합니다. Unix에서는 실행 권한을 설정하고, 교체 실패 시 rollback하며, managed binary SHA-256을 포함한 install record와 ledger event를 남깁니다.
- `rpotato backend start [--model <path>] [--ctx-size <tokens>]`는 명시된 로컬 모델 파일 또는 재검증된 지속 기본 모델과 선택적 runtime context limit으로 selected sidecar를 시작하고 app state 아래 pid record를 쓰며, stdout/stderr를 log file로 capture하고 `/health`를 기다린 뒤 startup timeout이면 child를 종료합니다.
- `rpotato backend status`는 pid record를 읽어 `running`, `stale`, `stopped`를 보고하고, process가 실행 중이면 health 상태도 포함합니다.
- `rpotato backend stop`은 active generation에 cancellation을 요청하고 terminal acknowledgement를 최대 5초 기다린 뒤 기록된 sidecar process를 종료합니다. Acknowledgement timeout은 성공한 cancellation이 아니라 `forced-sidecar-stop`으로 기록합니다.
- `rpotato backend verify-archive <path> --sha256 <hash>`는 로컬 backend archive bytes의 SHA-256을 검증하고 ledger event를 남깁니다.
- `rpotato backend health-check`는 selected host/port의 `/health`에 500ms timeout으로 HTTP 요청을 보내고 `healthy`, `unhealthy`, `unreachable` 중 하나로 보고합니다.
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]`는 항상 `/v1/chat/completions` SSE transport를 사용합니다. 기본 display는 filtering된 response를 모아 출력하고, `--stream`은 language guard를 통과한 완전한 text unit만 출력합니다. 전체 timeout 기본값은 30초, 범위는 1-300,000ms이며 resolution, connection, upload, response read 전체를 포함합니다. 누적 visible completion text는 2 MiB로 제한합니다.
- `rpotato backend cancel`은 app-data root에서 실행 중인 generation 하나에 generation-specific cancellation request를 기록합니다. Upload와 response read 중 최대 100ms 간격으로 cancellation을 확인하고 chat socket만 닫습니다. 명령은 정확한 terminal outcome record를 기다린 뒤 state를 정리하며 managed sidecar는 계속 실행합니다.
- Request는 `stream_options.include_usage=true`를 설정합니다. 정상 완료 시 final usage chunk를 `token_usage`에 projection하고, cancellation/timeout/failure로 해당 chunk를 받지 못하면 unknown을 유지해 잘못된 0-token row를 만들지 않습니다.
- HTTP body 전송 뒤 request retry는 0회입니다. Cancellation과 timeout은 normal non-resumable stream path를 사용하며 `X-Conversation-Id`를 보내지 않습니다.
- 지원하는 Qwen3.5와 Gemma 4 모델에는 각 model family의 non-thinking guidance에 따라 `chat_template_kwargs.enable_thinking=false`를 보냅니다: [Qwen3.5](https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode), [Gemma 4](https://ai.google.dev/gemma/docs/capabilities/thinking). `reasoning_content`는 폐기하고 incremental filter가 split된 `<think>` trace를 buffered/streaming display에 노출하지 않도록 막습니다. SSE event, HTTP chunk, 미완성 body buffer에는 명시적 제한이 있습니다. Upstream error payload는 고정 category로 축약하며 ledger에는 raw prompt 또는 raw response text를 기록하지 않습니다.
- Generation 시작, cancellation 요청, cancellation, timeout, failure, completion, stale lease cleanup을 terminal resource/model-run evidence와 함께 기록합니다. Exclusive create-new lock으로 lease 생성을 직렬화하고 읽을 수 있는 active record는 atomically publish하며, 기록된 owner process가 더 이상 살아 있지 않을 때만 stale ownership을 회수합니다. 별도 atomic terminal record가 자연 완료를 cancellation으로 오인하지 않도록 합니다. Ledger reader는 process 간 recoverable writer lease를 공유해 JSONL과 head의 안정된 pair만 검증합니다.
- `rpotato doctor`도 같은 discovery summary를 보여줍니다.
- Version detection은 install record와 현재 binary SHA-256이 선택된 release manifest와 일치하는 recorded managed binary에만 수행합니다. Env override binary는 실행하지 않고 skipped로 표시합니다.

Transport contract는 pinned upstream `llama.cpp b9982`를 기준으로 확인했습니다. Upstream은 SSE chat streaming을 문서화하고, normal stream의 response reader가 파기되면 task를 취소하며, `include_usage`가 활성화된 경우에만 final usage를 보냅니다. 2026-07-13 확인 출처: [chat completions](https://github.com/ggml-org/llama.cpp/blob/b9982/tools/server/README.md#post-v1chatcompletions), [response-reader lifecycle](https://github.com/ggml-org/llama.cpp/blob/b9982/tools/server/server-queue.h#L168-L208), [cancellation posting](https://github.com/ggml-org/llama.cpp/blob/b9982/tools/server/server-queue.cpp#L441-L460), [disconnect handling](https://github.com/ggml-org/llama.cpp/blob/b9982/tools/server/server-http.cpp#L520-L568), [final usage chunk](https://github.com/ggml-org/llama.cpp/blob/b9982/tools/server/server-task.cpp#L526-L537).

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
