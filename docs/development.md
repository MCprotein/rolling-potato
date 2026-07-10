# Development

This document defines the `rolling-potato` development environment and verification flow.

## Current State

The repository is currently in product-definition and early Rust runtime/CLI scaffold stage.

Implemented first boundaries:

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
- `rpotato monitor baseline`
- `rpotato monitor optimize`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
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

`rpotato init` initializes state layout, current state, append-only ledger, runtime evidence JSONL, and SQLite observability projection.

Session history is DB-backed for the current project. `session list`/`session history` read the SQLite projection, `session new` creates a fresh session identity, and `session resume <session-id>` or `resume <session-id>` writes the selected session to current state for subsequent commands. Full agent-loop transcript replay is not implemented yet.

Model install is still blocked for unverified candidates, but read-only evaluation preflight, benchmark planning, and evaluation artifact fetch are enabled through `rpotato model eval-plan <id>`, `rpotato model benchmark-plan <id>`, and `rpotato model fetch-candidate <id> --for-evaluation`. Backend install is enabled for supported OS/CPU pairs through the source-backed `llama.cpp` release `b9878` CPU artifact manifest. The model manifest schema, candidate state, source-backed license/source claims, unverified Qwen/Gemma GGUF artifact candidates, public benchmark source ledger, local registry surface, pre-download display plan, explicit evaluation fetch, local file SHA-256 verification, and failed/partial artifact cleanup surface are enabled. The Qwen/Gemma artifact candidates include pinned revision URLs, LFS SHA-256, and file size, but remain blocked from registry installation until local `llama.cpp b9878` smoke, RAM fit, mmproj need, and benchmark evidence are completed.

`run` now starts a canonical workflow artifact before calling the backend, persists the model response as a non-executable action, requires a structured source pointer/path/find/replace/pre-bound verification plan, rereads the authoritative source, and exits at `pending-approval` with a diff and exact `patch approve` command. `state resume` validates the workflow plus ledger checkpoint and does not call the backend again while approval is pending.

The checkpoint, ledger-chain, CSPRNG-token, atomic rollback, and stop-gate tests are platform-independent unit/state tests and run on Windows. `tests/patch_loop.rs` is currently Unix-only because its true subprocess fixture uses an executable Python fake sidecar and Unix process permissions. Therefore v0.29.0 does not claim Windows subprocess end-to-end coverage; Windows runs the portable state/runtime boundary while Unix additionally proves the multi-process CLI/backend flow.

If the one-time approval token was not captured, use `rpotato patch token-rotate
<proposal-id>` while the workflow is still pending. The command persists a new
credential hash and prints the replacement token once; it never reconstructs the
old token. Verification plans are argv-only and limited to `pwd` plus current-crate
`cargo test`, `cargo check`, `cargo fmt`, or `cargo clippy` variants.

`tui monitor` reads the same SQLite observability projection and shows schema, model/token counts, latest resource pressure, CPU percent, average/peak RSS, disk bytes, model latency, and average token throughput in a dependency-free terminal-safe layout. It remains read-only and delegates export/prune actions to monitor CLI commands.

`benchmark validate`, `benchmark record --fixture`, `benchmark run --fixture --prompt`, and `benchmark report --format jsonl` provide the benchmark harness surface. `record` remains metadata-only with `claim_state=not-comparable`; `run` executes a project-local prompt artifact through the running backend sidecar and records `claim_state=measured-locally`, a deterministic 0-3 local product score, the linked `model_run_id`, token/latency/resource summaries, and redacted reproducibility metadata. It still does not store raw prompt/source text in SQLite or claim public benchmark parity.

`patch preview --path <path> --find <text> --replace <text>` remains a diff-only standalone surface and cannot be approved, applied, or verified. Only proposals created by `run` bind workflow/action/proposal IDs, before/after hashes, and the exact verification command with mutation authority. `patch approve` validates every binding and a fresh source hash, persists approval before writing, applies through the no-clobber guard transaction with a rollback record, runs only the pre-bound policy-allowed argv verification, records hash-only evidence, and passes the stop gate only when applied source and evidence are fresh. `rpotato cancel` explicitly reconciles inconclusive applied phases without rerunning verification. Verification failure restores the original bytes; success and failure reports are deterministic Korean text.

`backend doctor` displays managed `llama.cpp` sidecar discovery, `RPOTATO_BACKEND_LLAMA_CPP_PATH` override, `RPOTATO_BACKEND_PORT` override, health URL, executable bit, install gate, and version detection for recorded managed binaries. `backend install-plan` displays the selected backend archive URL, SHA-256, size, and source. `backend install` downloads or reuses the cached archive, verifies it, extracts it in staging, places the release payload, writes an install record, and records a ledger event. `backend start --model <path> [--ctx-size <tokens>]` starts the selected sidecar with an explicit local model file and optional runtime context limit, captures stdout/stderr logs, writes a pid record, waits for `/health`, samples CPU/RSS/disk resource status, and kills the child on startup timeout. `backend status` reads the pid record, health state, and sampled resource pressure for running sidecars. `backend stop` removes stale records or terminates the recorded sidecar. `backend chat --prompt <text> [--max-tokens <tokens>]` samples the sidecar before model execution, blocks critical pressure, clamps degraded-pressure requests to the governor's effective max-token budget, calls `/v1/chat/completions`, disables Qwen3.5 thinking with `chat_template_kwargs.enable_thinking=false`, strips leaked `<think>` traces before display, records token usage without raw prompt/response text in the ledger, and records redacted resource samples. Env override binaries are not executed by `doctor`; they are executed only by explicit lifecycle commands.

`team status` is the read-only team-runtime admission preview. It reads the latest SQLite resource sample, reports requested/admitted lanes, shows whether a future dispatch would be parallel, sequential fallback, or blocked, and surfaces the latest `team.*` runtime ledger event for the current project. `team admit --lanes <count>` is the enforced gate: it records the admission decision in the ledger/SQLite projection and returns a blocked error on critical pressure, but still does not start workers or advance team stages. `team admit --lanes <count> --write <path> --command <command>` also preflights requested writes and tool commands with the shared policy engine; `ask` or `deny` blocks dispatch. `team admit --lanes <count> --write-owner <lane:path>` normalizes lane-owned write paths and blocks cross-lane conflicts before future worker launch. Blocked policy/ownership admission writes a project-local approval request under `.rpotato/approval-requests/`, and `tui approvals` lists those team requests beside patch proposal approvals. `team dispatch --lanes <count> --write-owner <lane:path>` rechecks normalized file ownership at the dispatch boundary, blocks cross-lane conflicts, records ledger/SQLite events, and can record failed-worker continuation with `--failed-lane <lane> --failure <reason>` without starting workers. `team governor --lanes <count> --context-tokens <tokens>` records context/model governor preflight decisions, clamps effective context tokens, and emits local model-tier route hints without selecting real model artifacts.

Plugin source snapshot, persistent registry, inspect, validate, enable/disable/remove are enabled. Import grants no execution authority; it records only permission reports and ledger events.

## Tech Stack

- Language: Rust
- CLI parser: manual parser based on std
- Runtime: separation between CLI surface and runtime core
- Required capabilities: hooks, skills, subagents, team runtime, TUI, local plugin adapter
- Backend: managed `llama.cpp` sidecar
- Model format: GGUF
- Primary OS targets: macOS, Windows

## Development Environment

Required tools:

- Git
- Rust stable toolchain
- SQLite runtime/library usable by `rusqlite`
- platform-specific C/C++ runtime needed by `llama.cpp`

Recommended tools:

- `rustfmt`
- `clippy`
- GitHub CLI

## Default Verification Commands

Use these as default verification:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

CLI smoke test examples:

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
cargo run -- monitor baseline
cargo run -- monitor optimize
cargo run -- team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
cargo run -- team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
cargo run -- monitor export --format jsonl
cargo run -- monitor export --format csv
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
# Intentional multi-GB evaluation download only; skip during routine smoke.
cargo run -- model fetch-candidate qwen3.5-4b --for-evaluation
cargo run -- model verify-file /path/to/model.gguf --sha256 <64-hex>
cargo run -- model cleanup-failed qwen3.5-4b --dry-run
cargo run -- model install qwen3.5-4b
cargo run -- plugin list
cargo run -- uninstall --dry-run --purge-cache
```

Final binary command is `rpotato`.

## Code Structure Direction

Current scaffold and planned module boundaries:

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
- `ontology`: typed graph store for Layer A facts, Layer B semantic assertions, source refs, drift/conflict state, and ontology query indexes
- `agent`: planner/executor/verifier/reporter loop
- `subagent`: bounded worker lifecycle
- `team`: staged multi-agent coordination
- `tui`: terminal interactive surface
- `policy`: command/path permission classifier and redaction surface
- `patch`: diff rendering and apply flow
- `evidence`: ledger, verification evidence, stop gate
- `observability`: SQLite migration/projection, token/resource metric schemas, monitoring export
- `guard`: Korean output validation

## Documentation Verification

For docs-only changes:

```sh
rg -n "<pattern-to-check>" README.md docs *.md
```

When links are added, verify target file existence.

For plugin adapter changes, additionally verify:

- only local directory import is allowed
- remote URL, marketplace, registry, catalog, and mirror sources are rejected
- shell, `bin/`, MCP, background, remote connector, and file-write capabilities are blocked by default
- import/enable/remove events are recorded in the ledger

## Commit And Push

After a work unit is verified, commit using Conventional Commits.

```text
docs(project): add open source operating docs
feat(cli): scaffold command router
fix(model): reject checksum mismatch
```

The default remote is `origin`, and the default branch is `main`.
