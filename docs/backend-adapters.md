# Backend Adapters

Backend adapters are the boundary that lets `rpotato` hide inference-backend differences.

## MVP Adapter

The MVP implements only the `llama.cpp` sidecar adapter.

Here, `llama.cpp` is an inference backend. It is not a model candidate and does not imply that Meta Llama-family models are default candidates. Model candidates and license checks follow [model-licenses.md](model-licenses.md) and [model-source-policy.md](model-source-policy.md).

The upstream `llama.cpp` LICENSE is MIT License. If a managed backend binary is bundled, downloaded, or installed, preserve the `llama.cpp` copyright/license notice. Source: https://github.com/ggml-org/llama.cpp/blob/master/LICENSE, checked 2026-06-25.

Reasons:

- GGUF support
- CPU execution support
- suitable for macOS and Windows first
- aligned with a small-model runtime
- lower packaging risk than native bindings

## Common Interface

Adapter capabilities:

- health check
- model metadata lookup
- chat completion
- streaming output
- cancellation
- context length reporting
- backend diagnostics

## `llama.cpp` Sidecar

Runtime core should manage:

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

Failure handling should narrow causes in Korean.

Examples:

- missing backend binary
- missing model file
- port in use
- checksum-failed model
- context length configuration error
- backend process crash

When users configure a custom backend path, that binary is user-owned. `rpotato uninstall --keep-cache` and `--purge-cache` remove only managed backend payloads downloaded by `rpotato`; they do not delete user-specified paths.

## Current Implementation

Phase 6 currently implements:

- `rpotato backend doctor` runs `llama.cpp` adapter discovery.
- Managed binary path is `backends/llama.cpp/llama-server` under app data root, or `llama-server.exe` on Windows.
- User override is `RPOTATO_BACKEND_LLAMA_CPP_PATH`.
- Port override is `RPOTATO_BACKEND_PORT`; default port is `17842`.
- Doctor output shows selected binary, selected source, executable bit, health URL, and install gate.
- `rpotato backend install-plan` renders the selected platform artifact, release URL, archive URL, archive name, file size, SHA-256, license source, and download path.
- The current manifest pins source-backed `llama.cpp` release `b9878` CPU artifacts for macOS arm64/x64, Linux arm64/x64, and Windows arm64/x64. Source: GitHub Releases API at https://api.github.com/repos/ggml-org/llama.cpp/releases/latest and release page https://github.com/ggml-org/llama.cpp/releases/tag/b9878, checked 2026-07-06.
- `backend install-plan` is `ready` only when the current OS/CPU pair has a recorded artifact; unsupported platforms remain blocked.
- `rpotato backend install` downloads or reuses the cached archive, verifies file size and SHA-256, extracts into a staging directory, places the release payload in the managed backend directory, sets the executable bit on Unix, rolls back failed replacement, writes an install record with the managed binary SHA-256, and records a ledger event.
- `rpotato backend start [--model <path>] [--ctx-size <tokens>]` starts the selected sidecar with an explicit local model file or the revalidated persistent default and an optional runtime context limit, writes a pid record under app state, captures stdout/stderr to log files, waits for `/health`, and kills the child on startup timeout.
- `rpotato backend status` reads the pid record, reports `running`, `stale`, or `stopped`, and includes health status when a process is running.
- `rpotato backend stop` requests cancellation of an active generation before removing stale records or terminating the recorded sidecar process, then records a ledger event.
- `rpotato backend verify-archive <path> --sha256 <hash>` verifies SHA-256 over local backend archive bytes and records a ledger event.
- `rpotato backend health-check` sends an HTTP request to `/health` on the selected host/port with a 500 ms timeout and reports `healthy`, `unhealthy`, or `unreachable`.
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]` always uses the `/v1/chat/completions` SSE transport. The default display buffers the filtered response; `--stream` flushes visible deltas as they arrive. The total timeout defaults to 30 seconds and is bounded to 1-300,000 ms.
- `rpotato backend cancel` writes a generation-specific cancellation request for the one active generation under the app-data root. The chat client polls cancellation at 100 ms intervals, closes only its HTTP socket, cleans its generation lease, and leaves the managed sidecar running.
- Requests set `stream_options.include_usage=true`. A completed final usage chunk is projected into `token_usage`; cancellation, timeout, or failure without that chunk remains unknown and does not create a false zero-token row.
- No request is retried after the HTTP body is sent. Cancellation and timeout use the normal non-resumable stream path and do not send `X-Conversation-Id`.
- For Qwen3.5 the request sends `chat_template_kwargs.enable_thinking=false`, following the Qwen model card's non-thinking mode guidance. `reasoning_content` is discarded and an incremental filter prevents split `<think>` traces from reaching buffered or streaming display. Ledger details never store raw prompt or raw response text.
- Generation start, cancellation request, cancellation, timeout, failure, completion, and stale-lease cleanup are recorded with terminal resource/model-run evidence. A generation lease is created atomically and reclaimed only when its recorded owner process is no longer alive.
- `rpotato doctor` shows the same discovery summary.
- Version detection runs only for recorded managed binaries whose install record and current binary SHA-256 match the selected release manifest. Env override binaries are skipped.

The transport contract is checked against pinned upstream `llama.cpp b9878`. Upstream documents SSE chat streaming, cancels a normal stream when the response reader is destroyed, and emits final usage only when `include_usage` is enabled. Sources, checked 2026-07-11: [chat completions](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/README.md#post-v1chatcompletions), [response-reader lifecycle](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.h#L168-L208), [cancellation posting](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.cpp#L441-L460), [disconnect handling](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-http.cpp#L521-L565), and [final usage chunk](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-task.cpp#L526-L537).

## Later Adapters

### LM Studio

Pros:

- Many users may already have it installed.
- Useful for demos and onboarding.

Constraints:

- Too dependent on an external app for the core runtime.

### Ollama

Pros:

- Large user base.
- Simple model-management experience.

Constraints:

- Heavy and opaque as the default runtime.
- Harder to enforce the tight policy needed for a small-model runtime.

### vLLM / SGLang

Pros:

- Suitable for GPU/server mode.

Constraints:

- Misaligned with the low-end local laptop MVP.
- Distant from the default Windows/macOS CPU experience.

## Adapter Acceptance Criteria

New adapters must:

- clearly show whether execution is local or remote
- not bypass privacy policy or command policy
- support streaming and cancellation
- provide backend diagnostics
- keep the Korean output guard at the final reporter stage
