# 릴리즈 노트

## v0.2.0 - Run Skeleton Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 managed `llama.cpp` sidecar 위에 첫 `rpotato run` vertical slice를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- context-aware `rpotato run "<task>"` skeleton
- skill, mode, signal, constraint로 deterministic request routing
- source pointer가 있는 bounded repository context packing
- runtime-owned action candidate와 next gate reporting
- structured action line 또는 인식 가능한 action text에서 실행 없는 model action parsing
- local SQLite observability projection에 model/token/latency metric 기록
- intent, context pack, action candidate, model action, backend chat, model run ledger event
- versioned backend/model user agent를 쓰도록 source policy 정리
- 새 `run` 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

최신 Qwen3.5 smoke는 `model action parse: heuristic-text`, `model action kind:
patch-proposal`, `model action executable now: no`, `guard: pass`, `finish
reason: stop`을 반환했습니다. 이는 현재의 실행 없는 runtime boundary와
observability path의 증거이지, patch 품질이나 autonomous tool use 통과 증거는
아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 계속 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 patch 적용, command 실행, model output의 승인된 action 처리를 하지 않습니다.
- model action parsing은 tolerant하고 실행하지 않습니다. 안정적인 structured action 생성과 approval UI는 후속 작업입니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직 설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.1.0 - 개발자 프리뷰

릴리즈 날짜: 2026-07-06

이 버전은 `rolling-potato`의 첫 개발자 프리뷰입니다. 초기 Rust runtime과
CLI scaffold를 위한 source-only release tag이며, stable runtime contract가
아닙니다. 모델 가중치, 외부 plugin package, prebuilt model/backend bundle은
포함하지 않습니다.

### 포함된 것

- `rpotato` Rust CLI scaffold
- project/app state 초기화
- SQLite projection 기반 session list/new/resume
- runtime ledger와 evidence validation surface
- command/path policy check와 credential redaction
- hook registry와 fail-closed hook result validation
- local plugin import/inspect/validate/enable/disable/remove surface
- monitoring status, model summary, export, dry-run prune surface
- source-backed Qwen/Gemma model candidate manifest와 evaluation gate
- size와 SHA-256 검증이 있는 evaluation-only model artifact fetch
- managed `llama.cpp b9878` backend install/start/status/stop/health surface
- `/v1/chat/completions` 기반 non-streaming backend chat smoke path
- `chat_template_kwargs.enable_thinking=false`를 쓰는 Qwen3.5 non-thinking smoke path
- 기본 영문 문서와 주요 문서의 한국어 번역

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

Qwen chat smoke는 managed `llama.cpp` sidecar를 통해 깨끗한 한국어 응답을
반환했습니다. 이는 backend/model 연결과 non-thinking chat path의 증거이지,
전체 모델 품질 통과 증거는 아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 intent normalization만 수행하며 full agent loop는
  구현되지 않았습니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직
  설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.
