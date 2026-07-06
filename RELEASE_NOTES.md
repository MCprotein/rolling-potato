# Release Notes

## v0.2.0 - Run Skeleton Preview

Release date: 2026-07-06

This release adds the first `rpotato run` vertical slice on top of the managed
`llama.cpp` sidecar. It is still a source-only developer preview: it does not
ship model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- Context-aware `rpotato run "<task>"` skeleton
- Deterministic request routing into skill, mode, signals, and constraints
- Bounded repository context packing with source pointers
- Runtime-owned action candidate and next gate reporting
- Non-executing model action parsing from structured action lines or recognized action text
- Model/token/latency metrics written to the local SQLite observability projection
- Ledger events for intent, context pack, action candidate, model action, backend chat, and model run
- Source policy cleanup for versioned backend/model user agents
- English and Korean documentation updates for the new `run` boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

The latest Qwen3.5 smoke returned `model action parse: heuristic-text`,
`model action kind: patch-proposal`, `model action executable now: no`,
`guard: pass`, and `finish reason: stop`. This proves the current non-executing
runtime boundary and observability path, not patch quality or autonomous tool use.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest still includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still does not apply patches, execute commands, or treat model
  output as an approved action.
- Model action parsing is tolerant and non-executing; robust structured action
  generation and approval UI are future work.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.1.0 - Developer Preview

Release date: 2026-07-06

This is the first `rolling-potato` developer preview. It is a source-only
release tag for the early Rust runtime and CLI scaffold. It is not a stable
runtime contract and does not ship model weights, external plugin packages, or
prebuilt model/backend bundles.

### Included

- Rust CLI scaffold for `rpotato`
- Project/app state initialization
- Session list/new/resume projection backed by SQLite
- Runtime ledger and evidence validation surfaces
- Command/path policy checks and credential redaction
- Hook registry and fail-closed hook result validation
- Local plugin import/inspect/validate/enable/disable/remove surfaces
- Monitoring status, model summary, export, and dry-run prune surfaces
- Source-backed Qwen/Gemma model candidate manifest and evaluation gates
- Evaluation-only model artifact fetch with size and SHA-256 verification
- Managed `llama.cpp b9878` backend install/start/status/stop/health surfaces
- Non-streaming backend chat smoke path through `/v1/chat/completions`
- Qwen3.5 non-thinking smoke path with
  `chat_template_kwargs.enable_thinking=false`
- English docs plus Korean translations for the main project documents

### Verified In This Release

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

The Qwen chat smoke returned a clean Korean response through the managed
`llama.cpp` sidecar. This proves backend/model connectivity and the
non-thinking chat path, not full model quality.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still performs intent normalization only; the full agent loop is
  not implemented.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.
