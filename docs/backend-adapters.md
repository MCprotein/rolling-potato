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

When users configure a custom backend path, that binary is user-owned. `rpotato uninstall --keep-cache` and `--purge-cache` remove only backend binaries downloaded by `rpotato`; they do not delete user-specified paths.

## Current Implementation

Phase 6 currently implements:

- `rpotato backend doctor` runs `llama.cpp` adapter discovery.
- Managed binary path is `backends/llama.cpp/llama-server` under app data root, or `llama-server.exe` on Windows.
- User override is `RPOTATO_BACKEND_LLAMA_CPP_PATH`.
- Port override is `RPOTATO_BACKEND_PORT`; default port is `17842`.
- Doctor output shows selected binary, selected source, executable bit, health URL, and install gate.
- `rpotato backend install-plan` renders release URL, archive name, file size, SHA-256, license source, and download path; it is currently blocked because no release manifest exists.
- `rpotato backend verify-archive <path> --sha256 <hash>` verifies SHA-256 over local backend archive bytes and records a ledger event.
- `rpotato backend health-check` sends an HTTP request to `/health` on the selected host/port with a 500 ms timeout and reports `healthy`, `unhealthy`, or `unreachable`.
- `rpotato doctor` shows the same discovery summary.
- Version detection is shown as `not-run` because unknown binaries are not executed yet.
- Managed backend download/install remains blocked until verified release URL and checksum manifest data exist.

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
