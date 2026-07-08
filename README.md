# rolling-potato

<p align="center">
  <strong>ENGLISH</strong> |
  <a href="README.ko.md">한국어</a>
</p>

`rolling-potato` is a local-first coding agent runtime for low-end and mid-range machines.

- CLI command: `rpotato`
- Tagline: `Local coding agents for potato PCs.`
- Korean positioning: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

## Direction

`rolling-potato` is not a thin wrapper that tries to imitate Claude Code or Codex with a weaker model.

The core assumption is the opposite: small local models can be useful, but they are fragile. The runtime must narrow their choices, enforce policy, preserve evidence, and manage failure modes.

> Small models need a small-model runtime, not just a smaller prompt.

The product is therefore not a prompt bundle. It is a local runtime responsible for:

- model installation and cache management
- local inference backend lifecycle
- repository context packing
- tool permission policy
- hooks and skills
- subagents and team execution
- Claude Code/Codex-style plugin adapters
- patch generation and verification
- retry control
- per-model token, latency, CPU, memory, and resource monitoring
- CLI and TUI surfaces
- Korean final-response validation

The `rpotato` CLI is the first user surface for this runtime. Users start work from the terminal, similar in spirit to Claude Code, Codex, or Gaja Code, but session state, tool permissions, ontology, context, the agent loop, and verification gates are owned by the runtime core, not by the CLI surface.

The long-term target is a local agent runtime that can replace Claude Code/Codex for practical coding workflows. Hooks, skills, subagents, team runtime, and TUI are first-class product capabilities, not optional extras.

## Users

The first audience is:

- Korean-speaking users
- users who find subscription coding agents expensive
- users with 16 GB RAM class low-end or mid-range laptops
- users who want code and model execution to remain local
- users who want coding assistance without needing to understand local LLM tooling first

The initial platform priority is macOS and Windows. Linux comes later by maintainer-led expansion or after the governance policy changes.

## MVP Scope

The first useful version should:

1. run through the `rpotato` command
2. download one recommended GGUF model after explicit user approval
3. start or reuse a local inference backend
4. converse in Korean
5. read a local repository and find the relevant files
6. propose a small patch
7. show the diff before applying it
8. run verification commands only after user approval
9. produce the final report in Korean only

Detailed acceptance criteria live in [docs/mvp.md](docs/mvp.md).

## Runtime Surface Sketch

The first MVP surface is the CLI. The TUI is required for a replacement-level runtime.

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato intent classify "테스트 실패 고쳐줘"
rpotato tui
rpotato tui monitor
rpotato tui sessions
rpotato tui transcript <session-id>
rpotato tui approvals
rpotato tui diff <proposal-id>
rpotato tui evidence
rpotato state reconcile
rpotato state resume
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato evidence validate logs/test.log
rpotato skill list
rpotato skill run fix-test
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato team status
rpotato team admit --lanes 2
rpotato team admit --lanes 2 --write README.md --command "cargo test"
rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs
rpotato model list
rpotato model knowledge
rpotato model knowledge inspect qwen3.5-4b
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

Plugin adapters do not use external marketplaces. Users import local Codex/Claude Code-style plugin directories, and `rpotato` inspects, validates, and enables normalized capabilities after reporting permission risks.

The expected initialization flow is:

1. The user runs `rpotato init` or `rpotato model install`.
2. The CLI surface forwards the request to the runtime core.
3. The runtime core checks OS, architecture, RAM, and available disk space.
4. The runtime core installs or verifies the managed `llama.cpp` sidecar.
5. The runtime core recommends a model from a source-verified manifest.
6. The CLI surface shows an explicit approval prompt.
7. The runtime core downloads the model with resume support.
8. The runtime core verifies the hash.
9. The runtime core registers the model in local config.
10. The runtime core starts or reuses the local inference backend.

Model weights are not bundled into the `rpotato` release artifact. The default path also does not require users to install `llama.cpp` globally; the runtime manages the sidecar. Removal is handled through `rpotato uninstall --keep-cache` or `rpotato uninstall --purge-cache`.

## Technical Direction

MVP defaults:

- implementation language: Rust
- default backend: `llama.cpp`
- model format: GGUF
- execution style: managed `llama-server` sidecar
- first surface: CLI
- required surface: TUI
- required runtime capabilities: hooks, skills, subagents, team runtime
- required compatibility boundary: Claude Code/Codex-style plugin adapters
- required monitoring store: local SQLite projection plus append-only ledger
- required model evidence index: model knowledge base, also called the LLM wiki in product discussions
- priority evaluation candidate: `Qwen3.5-4B` Q4_K_M GGUF from `unsloth/Qwen3.5-4B-GGUF`, unverified until local runtime validation
- comparison candidate: Google `Gemma 4 E4B` IT QAT q4_0 GGUF, unverified until local runtime validation

`llama.cpp` is a backend, not a model candidate. Model candidates are tracked only in the Qwen/Gemma lines currently documented for this project. License and artifact claims are recorded with sources in [docs/model-licenses.md](docs/model-licenses.md).

Architecture details are in [docs/architecture.md](docs/architecture.md). Runtime layers are in [docs/runtime-architecture.md](docs/runtime-architecture.md). Model evidence indexing is in [docs/model-knowledge-base.md](docs/model-knowledge-base.md), and terminology is fixed in [docs/glossary.md](docs/glossary.md).

## Important Constraint

User-facing natural-language output must be Korean. Code blocks, file paths, commands, and quoted logs are allowed exceptions, but final explanation text must not randomly mix English, Chinese, or Japanese.

This cannot be solved by model choice alone, so the runtime must enforce:

- final-response language leak checks
- separation of code blocks and natural-language blocks
- one regeneration attempt when mixed-language output is detected
- fail-closed Korean error output if regeneration still fails

## Current State

This repository is in the product-definition and early Rust runtime/CLI scaffold stage.

Implemented command surfaces:

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
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `rpotato tui evidence`
- `rpotato patch preview --path <path> --find <text> --replace <text>`
- `rpotato patch approve <proposal-id> --token <token> [--dry-run] [--verify-command <command>]`
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

`rpotato init` initializes the app data root and project-local `.rpotato/` state, including current state, append-only ledgers, runtime evidence JSONL, and a SQLite observability projection.

`state reconcile` preserves stale or corrupt current-state files before writing a fresh current-state file. `state resume` detects an active workflow pointer or records a no-op ledger event when there is nothing to resume.

`session list` and `session history` read the current project's session history from the SQLite projection. `session new` creates a new session identity and switches current state to that session. `session resume <session-id>` and `resume <session-id>` write the selected history entry into current state so later commands continue appending to the same session ledger and SQLite projection. Real model/backend agent-loop transcript replay and conversational continuation are still owned by the later agent-loop phase, which will consume this current-state pointer.

`evidence validate` checks that artifact pointers are local, project-relative paths that do not escape the project boundary.

`run` normalizes the user request into skill, mode, context, and evidence requirements, builds a bounded repository context pack with source pointers, prepares a runtime-owned action candidate and next gate, calls the running backend sidecar, and parses the model's structured action line or recognized action text without executing it. It records intent, context, action-candidate, model-action, and backend chat ledger events plus token/latency metrics in the local SQLite observability projection. It still does not apply patches, run commands, or treat model output as an approved action.

`intent classify`, `intent routes`, and `skill run` remain pre-execution surfaces: they normalize routing state and record ledger events without calling the model.

`tui`, `tui monitor`, `tui sessions`, `tui transcript <session-id>`, `tui approvals`, `tui diff <proposal-id>`, and `tui evidence` render read-only TUI beta surfaces using existing runtime state, the SQLite observability projection, project-local patch proposal records, and evidence store paths. They show project/session state, model/token/TPS summaries, CPU/RSS/disk resource pressure, session history, selected-session event timelines, approval queue records, proposal metadata, literal diffs, evidence counts, stop-gate result counts, stale policy, and read-only boundaries in terminal-friendly ASCII layouts. They do not approve, apply, resume, cancel, replay transcripts, pass or fail stop gates, or mutate workflows.

`policy` and `hooks` commands provide command/path permission decisions, credential redaction, lifecycle hook registry output, and fail-closed hook result validation. Real tool execution has not yet been wired behind this policy surface.

`patch preview` reads a project-local text file, renders a unified diff for a single explicit find/replace proposal, writes a project-local proposal record under `.rpotato/patch-proposals/`, and prints an approval token. `patch approve <proposal-id> --token <token> --dry-run` verifies the token and records the approval gate without modifying the target file. Without `--dry-run`, `patch approve` applies the approved proposal only when the current file SHA-256 still matches the previewed original SHA-256, writes a rollback record, verifies the applied SHA-256, and records a ledger event. `--verify-command <command>` runs an allow-listed simple argv verification command after apply; verification failure attempts rollback and is not reported as success.

`monitor export` emits the runtime ledger as JSONL or CSV. `monitor prune` is currently dry-run only.

`model list`, `model manifest`, `model inspect`, `model registry`, and `model download-plan` expose source-backed manifest structure, candidate status, benchmark source ledgers, local registry paths, and pre-download source/license/checksum fields. Qwen and Gemma now have source-recorded unverified GGUF artifact candidates, including pinned revision URLs, LFS SHA-256, and file size. `model eval-plan <id>` is the read-only local evaluation preflight: it checks source-backed artifact fields, app-data artifact presence, size/SHA-256 state, and the next smoke/benchmark command without downloading. `model benchmark-plan <id>` separates public benchmark reproduction conditions from local product benchmark fixtures and refuses score parity until artifact, quantization, backend, prompt, dataset, and scoring conditions are recorded together. `model fetch-candidate <id> --for-evaluation` is the explicit evaluation-only download path: it supports app-managed partial resume, verifies size and SHA-256, records a ledger event, and does not register the artifact as installed. `model verify-file` verifies SHA-256 over local file bytes and records a ledger event. `model cleanup-failed` targets only partial or failed artifacts under app data. `model install` still blocks registry installation until the candidate is promoted to `verified`; local `llama.cpp b9878` smoke, RAM fit, mmproj need, and benchmark evidence remain open.

`backend doctor` shows managed `llama.cpp` sidecar discovery, environment override path, port, health URL, executable bit, install gate state, and version detection for recorded managed binaries. `backend install-plan` selects a source-backed `llama.cpp` release `b9878` CPU artifact for supported OS/CPU pairs and displays the release URL, archive URL, SHA-256, size, license source, and download path. `backend install` downloads or reuses the cached archive, verifies size and SHA-256, extracts it in staging, places the release payload in the managed backend directory, sets executable permissions on Unix, rolls back failed replacement, writes an install record, and records a ledger event. `backend start --model <path> [--ctx-size <tokens>]` starts the selected sidecar with an explicit local model file and optional runtime context limit, records pid/log paths, waits for `/health`, samples CPU/RSS/disk resource status, and kills the child on startup timeout. `backend status` reads the sidecar pid record, health status, and latest sampled resource pressure for running sidecars. `backend stop` removes stale records or terminates the recorded sidecar. Env override binaries are not executed by `doctor`; they are executed only by explicit lifecycle commands. `backend verify-archive` verifies a local backend archive SHA-256. `backend health-check` checks `/health` on the selected host and port with a short timeout. `backend chat --prompt <text> [--max-tokens <tokens>]` samples the running sidecar before the request, blocks chat on critical resource pressure, clamps degraded-pressure requests to a smaller effective max-token budget, calls `/v1/chat/completions`, disables Qwen3.5 thinking with `chat_template_kwargs.enable_thinking=false`, strips any leaked `<think>` trace before display, records token usage without storing the raw prompt or response in the ledger, and records redacted resource samples.

Backend CPU/RSS/disk resource sampling is available through `backend start`, `backend status`, `backend chat`, `monitor status`, and the read-only `tui monitor` resource-pressure panel. The first runtime resource governor slice is active for backend chat. `team status` remains a read-only admission preview. `team admit --lanes <count>` is the enforced team admission gate: it records the decision in the ledger, admits parallel lanes on normal pressure, falls back to one sequential lane on unknown/degraded pressure, and blocks dispatch on critical pressure before any worker launch exists. `team admit` also accepts repeated `--write <path>`, `--write-owner <lane:path>`, and `--command <command>` preflight checks. Policy `ask` or `deny` results block dispatch, and duplicate normalized write ownership across lanes blocks dispatch before worker launch.

## Documentation

English documentation is the default repository surface. Korean translations are available separately:

- [Korean README](README.ko.md)
- [Korean documentation directory](docs/ko/)

Start with:

- [PLAN.md](PLAN.md)
- [ROADMAP.md](ROADMAP.md)
- [docs/runtime-architecture.md](docs/runtime-architecture.md)
- [docs/ontology-runtime.md](docs/ontology-runtime.md)
- [docs/architecture.md](docs/architecture.md)
- [docs/mvp.md](docs/mvp.md)

## Governance

This is a public open-source repository, but external code contributions and external PRs are not currently accepted. Bug reports, usability feedback, security reports, and model/license evidence may be accepted. See [GOVERNANCE.md](GOVERNANCE.md) and [SECURITY.md](SECURITY.md).
