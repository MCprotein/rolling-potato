# 릴리즈 노트

## v0.5.0 - Read-Only TUI Beta

릴리즈 날짜: 2026-07-07

이 릴리즈는 terminal-only 환경을 위한 첫 read-only TUI beta surface를 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui` overview dashboard
- `rpotato tui monitor` model/token monitoring view
- `rpotato tui sessions` full session id와 resume hint가 있는 session-history view
- SSH/Linux server 친화적인 dependency-free ASCII layout
- approval, patch apply, resume, cancel, workflow mutation을 수행하지 않는 read-only boundary
- TUI beta surface에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (133 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

TUI smoke는 project/session 상태, SQLite observability path, 기록된 model/token metric, session history, read-only beta boundary를 보여줬습니다.

### 알려진 제한

- TUI beta는 interactive event loop가 아니라 one-shot read-only render입니다.
- approval queue, diff viewer, transcript view, subagent/team status, plugin permission review, stop-gate evidence view는 후속 작업입니다.
- 첫 beta는 의도적으로 TUI framework dependency를 추가하지 않습니다. Interaction requirement가 안정된 뒤 더 풍부한 TUI crate가 필요한지 재검토할 수 있습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.4.0 - Approved Patch Apply

릴리즈 날짜: 2026-07-07

이 릴리즈는 patch approval surface를 dry-run gate 확인에서 승인된 patch apply,
rollback record, 선택적 verification command 실행까지 확장합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch approve <proposal-id> --token <token>`은 `--dry-run`이 없을 때 승인된 proposal을 적용합니다.
- apply 전 current file SHA-256 guard로 preview 이후 target file이 바뀐 stale proposal을 차단합니다.
- `.rpotato/patch-proposals/` 아래 rollback record를 생성합니다.
- write 이후 applied SHA-256을 검증합니다.
- `--verify-command <command>`는 apply 이후 allow 정책을 통과한 단순 argv verification command를 실행합니다.
- verification 실패 시 rollback을 시도하고 성공으로 보고하지 않습니다.
- 새 patch application 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (127 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke` scratch project smoke
- `rpotato patch preview --path README.md --find "Local coding agents for potato PCs." --replace "Local coding agents for potato PCs. Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --verify-command "rg Smoke README.md"`

Patch smoke는 `status: applied`, rollback record 생성, `verification status:
passed`, verification exit code `0`을 반환했습니다. Smoke는 repository working
tree가 아니라 `/private/tmp` project fixture에서 실행했습니다.

### 알려진 제한

- patch preview는 여전히 project-local UTF-8 text file 하나에 대한 명시적인 단일 find/replace proposal만 지원합니다.
- verification command는 policy가 allow한 단순 argv command로 제한됩니다. Shell syntax, quoting, pipe, redirect, environment expansion은 지원하지 않습니다.
- model action output은 아직 patch preview/apply에 자동 연결되지 않습니다.
- verification output interpretation과 final Korean task reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.3.0 - Patch Diff Approval Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 첫 patch diff display와 approval gate surface를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch preview --path <path> --find <text> --replace <text>`
- project-local text replacement 하나에 대한 unified diff rendering
- `.rpotato/patch-proposals/` 아래 project-local proposal record
- 생성된 proposal의 approval token 표시
- `rpotato patch approve <proposal-id> --token <token> --dry-run`
- patch 적용 없는 approval gate 검증과 ledger event 기록
- 새 patch 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (123 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path RELEASE_NOTES.md --find "Run Skeleton Preview" --replace "Run Skeleton Preview Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --dry-run`

Patch smoke는 `status: diff-ready`와 예상 unified diff를 반환했고, dry-run
approval에서는 `status: gate-passed`를 반환했습니다. Smoke 이후 target file에
Git diff가 없어 파일을 수정하지 않았음을 확인했습니다.

### 알려진 제한

- patch preview는 project-local UTF-8 text file 하나에 대해 명시적인 단일 find/replace proposal만 지원합니다.
- 이 릴리즈에서 patch approval은 dry-run 전용입니다. gate 결과를 기록하지만 patch를 적용하지 않습니다.
- model action에서 patch preview로 이어지는 agent-loop 통합은 후속 작업입니다.
- verification command execution, rollback handling, final Korean reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

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
