# 개발

이 문서는 `rolling-potato` 개발 환경과 검증 흐름을 정의합니다.

## 현재 상태

현재 저장소는 `v0.45.1` release를 가진 active pre-1.0 Rust runtime입니다.
읽기 쉬운 기능 지도는
[현재 기능 안내](current-capabilities.md)에서 먼저 확인하고, 아래 상세 snapshot은
구현 이력이 필요할 때만 펼쳐 보십시오.

<details>
<summary>v0.41 상세 구현 snapshot</summary>

대표 command surface:

- `rpotato doctor`
- `rpotato backend doctor`
- `rpotato backend install-plan`
- `rpotato backend install`
- `rpotato backend start [--model <path>] [--ctx-size <tokens>]`
- `rpotato backend status`
- `rpotato backend stop`
- `rpotato backend cancel`
- `rpotato backend verify-archive <path> --sha256 <hash>`
- `rpotato backend health-check`
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]`
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
- `rpotato continue`
- `rpotato continue <session-id>`
- `rpotato cancel`
- `rpotato evidence validate <artifact-pointer>`
- `rpotato skill list`
- `rpotato skill run <id> "<request>"`
- `rpotato policy schema`
- `rpotato policy check-command <command>`
- `rpotato policy check-path --read <path>`
- `rpotato policy check-path --write <path>`
- `rpotato policy redact <text>`
- `rpotato hooks list`
- `rpotato hooks validate-result <json>`
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato monitor baseline`
- `rpotato monitor optimize`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
- `rpotato monitor export --format html > rpotato-monitor.html`
- `rpotato monitor prune --before 30d --dry-run`
- `rpotato benchmark validate <fixture.json>`
- `rpotato benchmark record --fixture <fixture.json>`
- `rpotato benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]`
- `rpotato benchmark report --format jsonl`
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
- `rpotato uninstall --clean --dry-run`
- `rpotato uninstall --clean --yes`

`rpotato init`은 state layout, current-state, append-only ledger, runtime evidence JSONL, SQLite observability projection을 실제로 초기화합니다.

현재 project의 session history와 transcript 순서 권위는 canonical runtime ledger에 있습니다. `session list`/`session history`는 재생성 가능한 SQLite projection을 조회합니다. `session resume <session-id>`, `resume <session-id>`, `continue <session-id>`는 canonical transcript artifact와 source hash를 먼저 검증한 뒤 session을 선택하고 같은 session의 안전한 workflow checkpoint만 계속합니다. `continue`는 현재 선택을 재개하며 SQLite-only row는 권위가 아닙니다.

정적 `unverified` 후보는 엄격한 `model install` command에서 계속 차단됩니다. Source-backed 후보는 artifact bytes, 정확한 `backend.chat.completed` provenance, 실제 RAM/mmproj 상태, hash-pinned canonical adoption benchmark/prompt pair가 모두 재검증될 때만 local promotion할 수 있습니다. 별도로 interactive 최초 설정은 source/license/backend source와 artifact bytes를 재검증한 뒤 명시적인 사용자 선택을 제한된 `source-backed-manifest` evidence 상태로 등록할 수 있으며, RAM 적합성, capability 품질, benchmark parity를 주장하지 않습니다. `model default <id>`와 model 없는 `backend start`는 적용되는 evidence 상태를 재검증합니다. v0.30.0 strict 비교에서는 Qwen이 exact-response equality를 실패해 Gemma를 선택했으며, `model-eval.md`의 이 host-specific 결과는 public benchmark parity나 16GB evidence가 아닙니다.

`run`은 durable resume context를 검증하고 현재 요청과 resume context 전체에 source pointer 최대 4개·3,200자의 단일 공유 budget을 적용한 뒤 canonical workflow를 만들고 backend를 호출합니다. 이후 user, visible/normalized model, tool, evidence transcript artifact를 canonical하게 저장합니다. Resume는 최근 turn을 최대 8개·2,400자 안에서 재구성하며 stale hash는 workflow 생성 없이 fail-closed합니다. Hidden reasoning, raw backend response, raw source body, patch fragment, verification command 원문은 transcript artifact에서 제외합니다. `state resume`은 session 선택 전에 transcript/workflow/ledger checkpoint를 검증하고 pending approval에서 backend를 다시 호출하거나 불확실한 verification command를 자동 재실행하지 않습니다.

Checkpoint, ledger chain, CSPRNG token, atomic rollback, stop gate, streaming transport, generation-state lifecycle test는 플랫폼 독립적인 unit/state test입니다. `tests/inference/backend_lifecycle.rs`는 Rust fake sidecar를 compile해 Windows를 포함한 모든 release platform에서 실제 CLI child process를 실행합니다. `tests/patch_loop.rs`의 hostile fixture는 실행 가능한 Python fake sidecar와 Unix process permission을 사용하므로 계속 Unix 전용이며, Unix에서는 추가로 CLI process 사이의 timeout, 언어 거부, upstream-error redaction, sidecar-stop ordering을 검증합니다.

한 번만 표시되는 approval token을 확보하지 못했다면 workflow가 pending인 동안 `rpotato
patch token-rotate <proposal-id>`를 사용합니다. 이 명령은 새 credential hash를 저장하고
replacement token을 한 번만 출력하며 이전 token을 재구성하지 않습니다. Verification plan은
argv-only이며 `pwd`와 현재 crate 대상 `cargo test`, `cargo check`, `cargo fmt`, `cargo
clippy` 변형만 허용합니다.

`tui monitor`는 같은 SQLite observability projection을 읽고 schema, model/token count, 최신 resource pressure, CPU percent, average/peak RSS, disk bytes, model latency, average token throughput을 dependency-free terminal-safe layout으로 보여줍니다. Export/prune은 계속 monitor CLI operation입니다. v0.34.0부터 `tui`는 attached terminal에서 std-only interactive line controller로 자동 진입하고 `tui interactive`로 명시 선택할 수 있습니다. Interactive mutation은 runtime selection lease와 closed outcome table을 통과하며 terminal code는 workflow, ledger, SQLite를 직접 쓰지 않습니다. Native PTY test는 attached-terminal 선택, resize, no-echo secret input, control-byte-safe rendering을 검증하고 piped integration test는 EOF와 unknown-command no-op을 검증합니다.

`benchmark validate`, `benchmark record --fixture`, `benchmark run --fixture --prompt`, `benchmark report --format jsonl`은 benchmark harness surface입니다. `record`는 metadata-only `claim_state=not-comparable` 기록으로 남고, `run`은 project-local prompt artifact를 실행 중인 backend sidecar에 보내 `claim_state=measured-locally`, deterministic 0-3 local product score, 연결된 `model_run_id`, token/latency/resource summary, redacted reproducibility metadata를 기록합니다. SQLite에는 raw prompt/source text를 저장하지 않고 public benchmark parity도 주장하지 않습니다.

`patch preview --path <path> --find <text> --replace <text>`는 approve/apply/verification을 할 수 없는 diff-only standalone surface입니다. Mutation 권위는 `run`이 만든 workflow/action/proposal ID, before/after hash, 정확한 verification plan binding에만 있습니다. `patch approve`는 모든 binding과 fresh source hash를 검증하고 patch approval을 side effect 전에 저장한 뒤 no-clobber guard transaction과 rollback record로 적용합니다. 이후 command를 실행하지 않고 `pending-verification-approval`에서 멈추며 별도의 일회성 credential을 발급합니다. `patch verify`가 두 번째 gate를 검증한 뒤 policy가 허용한 pre-bound argv plan만 실행하고 hash-only evidence를 기록하며 applied source와 evidence가 fresh할 때만 stop gate를 통과합니다. `rpotato cancel`은 verification을 재실행하지 않고 applied 또는 inconclusive phase를 명시적으로 정리합니다. Verification 실패는 original byte를 복원하고 성공/실패 보고는 deterministic 한국어 text로 생성합니다.

`backend doctor`는 관리형 `llama.cpp` sidecar discovery, `RPOTATO_BACKEND_LLAMA_CPP_PATH` override, `RPOTATO_BACKEND_PORT` override, health URL, executable bit, install gate, recorded managed binary의 version detection을 표시합니다. `backend install-plan`은 선택된 backend archive URL, SHA-256, size, source를 표시합니다. `backend install`은 archive를 다운로드하거나 cache를 재사용하고, 검증 후 staging에서 압축을 풀어 release payload를 배치하며 install record와 ledger event를 남깁니다. `backend start [--model <path>] [--ctx-size <tokens>]`는 명시된 로컬 모델 파일 또는 재검증된 지속 기본 모델과 선택적 runtime context limit으로 selected sidecar를 시작하고 stdout/stderr log와 pid record를 남긴 뒤 `/health`를 기다리며 CPU/RSS/disk resource status를 sampling하고, startup timeout이면 child를 종료합니다. `backend status`는 pid record, health 상태, 실행 중인 sidecar의 sampled resource pressure를 읽고, `backend stop`은 cancellation을 요청하고 terminal acknowledgement를 기다린 뒤 sidecar를 종료합니다. `backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]`는 모델 실행 전에 resource governor를 적용하고 `/v1/chat/completions` SSE를 소비합니다. `--stream`은 language guard를 통과한 완전한 text unit을 출력하고 기본값은 response를 모아서 출력합니다. 기본 30초, 최대 300초 timeout은 resolution, connection, upload, response read 전체를 포함합니다. Upload와 read는 최대 100ms 간격으로 cancellation을 확인하고 request는 재시도하지 않습니다. `backend cancel`은 client connection을 닫아 active generation 하나를 중단하고 정확한 terminal outcome을 기다리며 sidecar는 계속 실행합니다. 정상 완료된 final usage만 projection하며 중단된 usage는 unknown으로 유지합니다. Upstream error detail과 raw prompt/response text는 저장하지 않습니다. Env override binary는 `doctor`가 실행하지 않으며 명시적인 lifecycle 명령에서만 실행됩니다.

`team status`는 read-only team-runtime admission preview입니다. 최신 SQLite resource sample을 읽어 requested/admitted lane을 표시하고 향후 dispatch가 parallel, sequential fallback, blocked 중 어디에 해당하는지 보여주며 현재 project의 최신 `team.*` runtime ledger event도 표시합니다. `team plan --manifest <path>`는 canonical manifest를 정확한 active parent에 binding하고, `team execute --team <id>`는 이를 소비해 normal pressure에서는 모든 member를 병렬 실행하고 unknown/degraded pressure에서는 모두 순차 실행하며 result는 후속 reconciliation 전까지 parent에 merge하지 않습니다. `team reconcile --team <id>`는 정확한 completed worker 집합과 immutable evidence를 검증하고 unresolved validation gap을 parent mutation 전에 차단하며, 모든 evidence를 parent checkpoint 하나로 merge한 뒤 evidence-required stop gate를 통과한 경우에만 완료합니다. `team cancel --team <id>`는 모든 active 또는 이후 sequential worker가 관측하는 durable manifest/parent-bound marker를 쓰고 team을 `cancelled`로 전진시킵니다. `team admit --lanes <count>`는 standalone enforced gate입니다. Admission decision을 ledger/SQLite projection에 기록하고 critical pressure에서는 blocked error를 반환하지만 worker를 시작하거나 team stage를 전진시키지는 않습니다. `team admit --lanes <count> --write <path> --command <command>`는 요청 write와 tool command도 공통 policy engine으로 preflight하며, `ask` 또는 `deny`는 dispatch를 차단합니다. `team admit --lanes <count> --write-owner <lane:path>`는 lane별 write path를 정규화하고 향후 worker launch 전에 cross-lane conflict를 차단합니다. 차단된 policy/ownership admission은 `.rpotato/approval-requests/` 아래 project-local approval request를 기록하지만, `tui approvals`는 해당 directory를 scan하지 않고 대응하는 canonical ledger event와 active workflow-bound patch proposal만 읽습니다. `team dispatch --lanes <count> --write-owner <lane:path>`는 dispatch boundary에서 normalized file ownership을 다시 검사하고 cross-lane conflict를 차단하며 ledger/SQLite event를 기록합니다. `--failed-lane <lane> --failure <reason>`으로 failed-worker continuation도 기록할 수 있지만 worker를 시작하지 않습니다. `team governor --lanes <count> --context-tokens <tokens>`는 context/model governor preflight decision을 기록하고, effective context token을 clamp하며, 실제 model artifact를 선택하지 않고 local model-tier route hint를 냅니다.

Plugin source snapshot, persistent registry, inspect, validate, enable/disable/remove는 활성화되어 있습니다. Import는 실행 권한을 부여하지 않고 permission report와 ledger event만 남깁니다.

</details>

## 기술 스택

- 언어: Rust
- CLI parser: std 기반 수동 parser
- Runtime: CLI surface와 runtime core를 분리하는 방향
- 필수 capability: hooks, skills, subagents, team runtime, TUI, local plugin adapter
- Backend: managed `llama.cpp` sidecar
- 모델 format: GGUF
- 우선 지원 OS: macOS, Linux, Windows

## 개발 환경

필수 도구:

- Git
- `rust-toolchain.toml`과 `mise.toml`에 고정된 Rust 1.97.0 stable toolchain
- SQLite runtime/library usable by `rusqlite`
- platform-specific C/C++ runtime needed by `llama.cpp`

권장 도구:

- `rustfmt`
- `clippy`
- GitHub CLI

저장소 pin이 build source of truth입니다. Hosted runner에 우연히 설치된 compiler에
의존하지 말고 `rust-toolchain.toml`, `mise.toml`, `Cargo.toml`의
`package.rust-version`을 함께 갱신합니다.

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
cargo run -- backend cancel
cargo run -- backend verify-archive /path/to/llama.cpp.zip --sha256 <64-hex>
cargo run -- backend health-check
cargo run -- backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64 --stream --timeout-ms 30000
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
cargo run -- continue
cargo run -- continue <session-id>
cargo run -- evidence validate .rpotato/evidence/smoke.txt
cargo run -- skill list
cargo run -- skill run fix-test "tests/api.rs의 실패를 고쳐줘"
cargo run -- policy schema
cargo run -- policy check-command cargo test
cargo run -- policy check-path --write src/main.rs
cargo run -- policy redact "token=secret"
cargo run -- hooks list
cargo run -- hooks validate-result '{"status":"allow"}'
cargo run -- monitor status
cargo run -- monitor models
cargo run -- monitor baseline
cargo run -- monitor optimize
cargo run -- team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
cargo run -- team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
cargo run -- monitor export --format jsonl
cargo run -- monitor export --format csv
cargo run -- monitor export --format html > rpotato-monitor.html
cargo run -- monitor prune --before 30d --dry-run
cargo run -- benchmark validate benchmarks/fixtures/sample.json
cargo run -- benchmark record --fixture benchmarks/fixtures/sample.json
cargo run -- benchmark validate benchmarks/fixtures/executable-smoke.json
cargo run -- benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32
cargo run -- benchmark report --format jsonl
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
- `plugins`: local Codex/Claude Code plugin import, inspect, validate, enable/disable/remove와 instruction-only skill/command 실행
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
- 지원하지 않는 Claude Code manifest/frontmatter/layout semantic이 명시적으로 보고되는지
- canonical default-path Claude Code skill/command만 native read-only runtime에 진입하는지
- import/enable/remove 이벤트가 ledger에 기록되는지

## 커밋과 푸시

작업 단위가 검증되면 Conventional Commit 형식으로 커밋합니다.

```text
docs(project): add open source operating docs
feat(cli): scaffold command router
fix(model): reject checksum mismatch
```

이 저장소의 기본 원격은 `origin`이고 기본 branch는 `main`입니다.
