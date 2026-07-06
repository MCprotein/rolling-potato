# Release Notes

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

