# Development

This document defines the `rolling-potato` development environment and verification flow.

## Current State

The repository is currently in product-definition and early Rust runtime/CLI scaffold stage.

Implemented first boundaries:

- `rpotato doctor`
- `rpotato backend doctor`
- `rpotato backend install-plan`
- `rpotato backend verify-archive <path> --sha256 <hash>`
- `rpotato backend health-check`
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

Model/backend downloads are not enabled yet. The model manifest schema, candidate state, source-backed license/source claims, public benchmark source ledger, local registry surface, pre-download display plan, local file SHA-256 verification, and failed/partial artifact cleanup surface are enabled. Without verified artifact URL, provider terms, checksum, file size, and backend compatibility, runtime core blocks downloads and records a ledger event.

`backend doctor` displays managed `llama.cpp` sidecar discovery, `RPOTATO_BACKEND_LLAMA_CPP_PATH` override, `RPOTATO_BACKEND_PORT` override, health URL, executable bit, and install gate. Version detection is `not-run` because unknown binaries are not executed yet.

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
cargo run -- backend verify-archive /path/to/llama.cpp.zip --sha256 <64-hex>
cargo run -- backend health-check
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
- `ontology`: Layer A facts and Layer B semantic ontology
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
