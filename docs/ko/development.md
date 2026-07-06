# 개발

이 문서는 `rolling-potato` 개발 환경과 검증 흐름을 정의합니다.

## 현재 상태

현재 저장소는 제품 정의와 초기 Rust runtime/CLI scaffold 단계입니다.

구현된 첫 경계:

- `rpotato doctor`
- `rpotato backend doctor`
- `rpotato backend install-plan`
- `rpotato backend install`
- `rpotato backend start --model <path> [--ctx-size <tokens>]`
- `rpotato backend status`
- `rpotato backend stop`
- `rpotato backend verify-archive <path> --sha256 <hash>`
- `rpotato backend health-check`
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>]`
- `rpotato cache status`
- `rpotato config`
- `rpotato init`
- `rpotato run "<request>"`
- `rpotato intent classify "<request>"`
- `rpotato intent routes`
- `rpotato state`
- `rpotato state reconcile`
- `rpotato state resume`
- `rpotato session list`
- `rpotato session history`
- `rpotato session resume <session-id>`
- `rpotato session new`
- `rpotato resume`
- `rpotato resume <session-id>`
- `rpotato cancel`
- `rpotato evidence validate <artifact-pointer>`
- `rpotato skill list`
- `rpotato skill run <id>`
- `rpotato policy schema`
- `rpotato policy check-command <command>`
- `rpotato policy check-path --read <path>`
- `rpotato policy check-path --write <path>`
- `rpotato policy redact <text>`
- `rpotato hooks list`
- `rpotato hooks validate-result <json>`
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
- `rpotato monitor prune --before 30d --dry-run`
- `rpotato model list`
- `rpotato model manifest`
- `rpotato model inspect <id>`
- `rpotato model registry`
- `rpotato model download-plan <id>`
- `rpotato model eval-plan <id>`
- `rpotato model benchmark-plan <id>`
- `rpotato model fetch-candidate <id> --for-evaluation`
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`
- `rpotato plugin import --from codex <local-path> --dry-run`
- `rpotato plugin import --from claude-code <local-path> --dry-run`
- `rpotato plugin import --from codex <local-path>`
- `rpotato plugin import --from claude-code <local-path>`
- `rpotato plugin list`
- `rpotato plugin inspect <id>`
- `rpotato plugin validate <id>`
- `rpotato plugin enable <id>`
- `rpotato plugin disable <id>`
- `rpotato plugin remove <id> --keep-data`
- `rpotato plugin remove <id> --purge-data`
- `rpotato uninstall --keep-cache`
- `rpotato uninstall --purge-cache`

`rpotato init`은 state layout, current-state, append-only ledger, runtime evidence JSONL, SQLite observability projection을 실제로 초기화합니다.

세션 히스토리는 현재 project 기준으로 SQLite에 저장됩니다. `session list`/`session history`는 SQLite projection을 읽고, `session new`는 새 session identity를 만들며, `session resume <session-id>` 또는 `resume <session-id>`는 선택한 session을 current-state에 기록해 이후 명령이 그 session으로 이어지게 합니다. 전체 agent loop transcript replay는 아직 구현하지 않았습니다.

모델 install은 아직 unverified 후보에 대해 차단되지만, read-only 평가 preflight, benchmark planning, 평가용 artifact fetch는 `rpotato model eval-plan <id>`, `rpotato model benchmark-plan <id>`, `rpotato model fetch-candidate <id> --for-evaluation`으로 활성화되어 있습니다. Backend install은 지원 OS/CPU 조합에 대해 source-backed `llama.cpp` release `b9878` CPU artifact manifest를 통해 활성화되어 있습니다. 모델 manifest schema, 후보 상태, source-backed license/source claim, `unverified` Qwen/Gemma GGUF artifact 후보, 공개 benchmark source ledger, local registry surface, 다운로드 전 표시 plan, 명시적 평가용 fetch, 로컬 파일 SHA-256 검증, failed/partial artifact cleanup surface는 활성화되어 있습니다. Qwen/Gemma artifact 후보에는 pinned revision URL, LFS SHA-256, file size가 들어갔지만 local `llama.cpp b9878` smoke, RAM fit, mmproj 필요 여부, benchmark evidence가 끝나기 전까지 registry 설치는 차단됩니다.

`run`은 이제 deterministic routing, intent ledger 기록, 실행 중인 backend sidecar 호출, model-response-only agent-loop 골격, SQLite token/latency metric 기록까지 수행합니다. Repository file read, context packing, patch 적용, command 실행, 승인된 action 처리는 후속 phase로 남아 있습니다.

`backend doctor`는 관리형 `llama.cpp` sidecar discovery, `RPOTATO_BACKEND_LLAMA_CPP_PATH` override, `RPOTATO_BACKEND_PORT` override, health URL, executable bit, install gate, recorded managed binary의 version detection을 표시합니다. `backend install-plan`은 선택된 backend archive URL, SHA-256, size, source를 표시합니다. `backend install`은 archive를 다운로드하거나 cache를 재사용하고, 검증 후 staging에서 압축을 풀어 release payload를 배치하며 install record와 ledger event를 남깁니다. `backend start --model <path> [--ctx-size <tokens>]`는 명시된 로컬 모델 파일과 선택적 runtime context limit으로 selected sidecar를 시작하고 stdout/stderr log와 pid record를 남긴 뒤 `/health`를 기다리며, startup timeout이면 child를 종료합니다. `backend status`는 pid record와 health 상태를 읽고, `backend stop`은 stale record를 제거하거나 기록된 sidecar를 종료합니다. `backend chat --prompt <text> [--max-tokens <tokens>]`는 실행 중인 sidecar의 `/v1/chat/completions`를 호출하고 Qwen3.5 thinking을 `chat_template_kwargs.enable_thinking=false`로 끄며, 누수된 `<think>` trace를 표시 전에 제거하고 raw prompt/response text 없이 token 사용량만 ledger에 기록합니다. Env override binary는 `doctor`가 실행하지 않으며 명시적인 lifecycle 명령에서만 실행됩니다.

Plugin source snapshot, persistent registry, inspect, validate, enable/disable/remove는 활성화되어 있습니다. Import는 실행 권한을 부여하지 않고 permission report와 ledger event만 남깁니다.

## 기술 스택

- 언어: Rust
- CLI parser: std 기반 수동 parser
- Runtime: CLI surface와 runtime core를 분리하는 방향
- 필수 capability: hooks, skills, subagents, team runtime, TUI, local plugin adapter
- Backend: managed `llama.cpp` sidecar
- 모델 format: GGUF
- 우선 지원 OS: macOS, Windows

## 개발 환경

필수 도구:

- Git
- Rust stable toolchain
- SQLite runtime/library usable by `rusqlite`
- platform-specific C/C++ runtime needed by `llama.cpp`

권장 도구:

- `rustfmt`
- `clippy`
- GitHub CLI

## 기본 검증 명령

다음 명령을 기본 검증으로 사용합니다.

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

CLI smoke test 예시:

```sh
cargo run -- doctor
cargo run -- backend doctor
cargo run -- backend install-plan
cargo run -- backend install
cargo run -- backend start --model /path/to/model.gguf --ctx-size 4096
cargo run -- backend status
cargo run -- backend stop
cargo run -- backend verify-archive /path/to/llama.cpp.zip --sha256 <64-hex>
cargo run -- backend health-check
cargo run -- backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64
cargo run -- init
cargo run -- run "테스트 실패 고쳐줘"
cargo run -- intent classify "리뷰해줘"
cargo run -- intent routes
cargo run -- config
cargo run -- state
cargo run -- state reconcile
cargo run -- state resume
cargo run -- session new
cargo run -- session list
cargo run -- session history
cargo run -- session resume <session-id>
cargo run -- resume
cargo run -- resume <session-id>
cargo run -- evidence validate .rpotato/evidence/smoke.txt
cargo run -- skill list
cargo run -- skill run fix-test
cargo run -- policy schema
cargo run -- policy check-command cargo test
cargo run -- policy check-path --write src/main.rs
cargo run -- policy redact "token=secret"
cargo run -- hooks list
cargo run -- hooks validate-result '{"status":"allow"}'
cargo run -- monitor status
cargo run -- monitor models
cargo run -- monitor export --format jsonl
cargo run -- monitor export --format csv
cargo run -- monitor prune --before 30d --dry-run
cargo run -- model list
cargo run -- model manifest
cargo run -- model inspect qwen3.5-4b
cargo run -- model registry
cargo run -- model download-plan qwen3.5-4b
cargo run -- model eval-plan qwen3.5-4b
cargo run -- model benchmark-plan qwen3.5-4b
# 의도적인 multi-GB 평가 다운로드입니다. routine smoke에서는 건너뜁니다.
cargo run -- model fetch-candidate qwen3.5-4b --for-evaluation
cargo run -- model verify-file /path/to/model.gguf --sha256 <64-hex>
cargo run -- model cleanup-failed qwen3.5-4b --dry-run
cargo run -- model install qwen3.5-4b
cargo run -- plugin list
cargo run -- uninstall --dry-run --purge-cache
```

최종 binary 명령은 `rpotato`입니다.

## 코드 구조 방향

현재 scaffold와 예정 module 경계:

- `cli`: command parsing and output
- `runtime`: state, policy, ontology, agent loop orchestration
- `intent`: deterministic request-to-skill/mode normalization
- `ledger`: append-only runtime/session ledger and redaction before persistence
- `state`: current-state, project/session identity, cancel/no-op event recording
- `evidence`: project-bound artifact pointer validation and stale policy summary
- `skill`: built-in skill registry and invocation normalization
- `hooks`: lifecycle hook registry and fail-closed result validation
- `skills`: reusable runtime capabilities
- `plugins`: local Codex/Claude Code plugin import, inspect, validate, enable/disable/remove
- `config`: local config paths and serialization
- `model`: manifest, download, checksum, registry
- `backend`: backend adapter trait and `llama.cpp` implementation
- `repo`: project file discovery and context packing
- `ontology`: Layer A fact, Layer B semantic assertion, source ref, drift/conflict state, ontology query index를 저장하는 typed graph store
- `agent`: planner/executor/verifier/reporter loop
- `subagent`: bounded worker lifecycle
- `team`: staged multi-agent coordination
- `tui`: terminal interactive surface
- `policy`: command/path permission classifier and redaction surface
- `patch`: diff rendering and apply flow
- `evidence`: ledger, verification evidence, stop gate
- `observability`: SQLite migration/projection, token/resource metric schemas, monitoring export
- `guard`: Korean output validation

## 문서 변경 검증

문서만 변경한 경우:

```sh
rg -n "<확인할-오타-패턴>" README.md docs *.md
```

링크가 추가되면 파일 존재 여부를 확인합니다.

Plugin adapter 변경 시 추가로 확인합니다.

- local directory import만 허용되는지
- remote URL, marketplace, registry, catalog, mirror source가 거부되는지
- shell, `bin/`, MCP, background, remote connector, file write capability가 기본 차단되는지
- import/enable/remove 이벤트가 ledger에 기록되는지

## 커밋과 푸시

작업 단위가 검증되면 Conventional Commit 형식으로 커밋합니다.

```text
docs(project): add open source operating docs
feat(cli): scaffold command router
fix(model): reject checksum mismatch
```

이 저장소의 기본 원격은 `origin`이고 기본 branch는 `main`입니다.
