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

The current binary priority is macOS, Linux, and Windows through maintainer-led GitHub Release artifacts.

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
rpotato continue
rpotato continue <session-id>
rpotato evidence validate logs/test.log
rpotato skill list
rpotato skill run fix-test
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato team status
rpotato team admit --lanes 2
rpotato team admit --lanes 2 --write README.md --command "cargo test"
rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
rpotato model list
rpotato model manifest
rpotato model inspect qwen3.5-4b
rpotato model eval-plan qwen3.5-4b
rpotato model benchmark-plan qwen3.5-4b
rpotato model fetch-candidate qwen3.5-4b --for-evaluation
rpotato model promote qwen3.5-4b --evidence evidence/qwen3.5-4b-local.json
rpotato model install qwen3.5-4b
rpotato model default gemma-4-e4b
rpotato model default
rpotato backend start --ctx-size 4096
rpotato backend doctor
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
rpotato benchmark validate benchmarks/fixtures/sample.json
rpotato benchmark record --fixture benchmarks/fixtures/sample.json
rpotato benchmark report --format jsonl
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
9. The runtime core promotes the candidate only after local smoke, RAM/mmproj,
   and measured benchmark evidence.
10. The runtime core registers the model in local config.
11. The runtime core starts or reuses the local inference backend.

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
- `rpotato patch approve <proposal-id> --token <token> [--dry-run]`
- `rpotato patch verify <proposal-id> --token <token>`
- `rpotato patch token-rotate <proposal-id>`
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato monitor baseline`
- `rpotato monitor optimize`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
- `rpotato monitor prune --before 30d --dry-run`
- `rpotato ontology status`
- `rpotato ontology seed`
- `rpotato ontology inspect`
- `rpotato ontology context --query <text>`
- `rpotato ontology reread <source-pointer>`
- `rpotato ontology export --format json`
- `rpotato ontology export --format jsonl`
- `rpotato ontology import --file <path> --dry-run`
- `rpotato benchmark validate <fixture.json>`
- `rpotato benchmark record --fixture <fixture.json>`
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
- `rpotato model promote <id> --evidence <file>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`
- `rpotato model default [<id>]`
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

`rpotato init` initializes the app data root and project-local `.rpotato/` state, including current state, append-only ledgers, runtime evidence JSONL, a SQLite observability projection, and the project-local ontology store/schema. It seeds deterministic Layer A facts from source-backed project files without storing raw source text in the ontology store.

`state reconcile` preserves stale or corrupt current-state files before writing a fresh current-state file. `state resume` and `continue` validate and reconstruct the selected session's bounded durable context before continuing a safe active workflow checkpoint. Pending approval does not call the backend again, and an uncertain backend request or verification command is never automatically repeated.

`session list` and `session history` render the current project's session history through a SQLite projection rebuilt from the canonical runtime ledger. `session new` creates a new session identity and switches current state to that session. `session resume <session-id>`, `resume <session-id>`, and `continue <session-id>` require canonical-ledger ownership, validate durable transcript artifacts and source hashes before changing current state, then continue only a matching safe workflow checkpoint. A SQLite-only row can never authorize resume, and a non-terminal workflow owned by another session blocks selection without mutation.

`evidence validate` checks that artifact pointers are local, project-relative paths that do not escape the project boundary.

`run` normalizes the user request into skill, mode, context, and evidence requirements, reconstructs up to 8 recent durable turns within 2,400 characters, and shares one 4-pointer/3,200-character source budget across current-request and resumed context before creating a workflow or calling the backend sidecar. It parses the result into a runtime-owned typed action without executing model text. Canonical transcript artifacts store the user turn, visible or normalized model result, normalized tool record, and evidence record; source contents and patch fragments remain pointers plus SHA-256, and hidden reasoning/raw backend responses are excluded. SQLite `transcript_records` is a rebuildable ordered projection, not resume authority. Read-only actions finish through a guarded Korean report. A valid patch action persists a restart-safe workflow and proposal, rereads the authoritative source, and stops with the exact `patch approve` gate.

`intent classify`, `intent routes`, and `skill run` remain pre-execution surfaces: they normalize routing state and record ledger events without calling the model.

`tui`, `tui monitor`, `tui sessions`, `tui transcript <session-id>`, `tui approvals`, `tui diff <proposal-id>`, and `tui evidence` render read-only TUI beta surfaces. The transcript view validates and shows durable user/model/tool/evidence turns alongside the ledger event timeline. It excludes hidden model responses, source-file bodies, patch fragments, and verification-command text. TUI views do not approve, apply, resume, cancel, pass or fail stop gates, or mutate workflows.

`policy` and `hooks` commands provide command/path permission decisions, credential redaction, lifecycle hook registry output, and fail-closed hook result validation. Real tool execution has not yet been wired behind this policy surface.

`patch preview` reads a project-local text file, renders a unified diff for a single explicit find/replace proposal, and writes a project-local record under `.rpotato/patch-proposals/`. This standalone surface is diff-only and cannot be approved, applied, or verified. Only a workflow proposal created by `run` can pass `patch approve`. `patch approve <proposal-id> --token <token> --dry-run` validates the patch-application gate without modifying the target file. Without `--dry-run`, `patch approve` applies the workflow proposal only when every binding and the current source SHA-256 remain valid, then emits a separate one-time verification credential without running the command. `patch verify <proposal-id> --token <token>` approves and runs only the pre-bound, policy-allowed argv verification plan. Verification failure attempts rollback and is never reported as success. `patch token-rotate` rotates the credential for the gate currently awaiting approval; neither credential is stored in plaintext or redisplayed after its initial delivery.

`monitor baseline` aggregates local ledger/SQLite projection metrics into a read-only performance baseline report with p50/p95 latency, average tokens/sec, context clamp count, peak RSS, pressure-state distribution, and model/backend/session grouping. It does not store raw prompt/source text and does not choose model artifacts. `monitor optimize` reads those local metrics plus `measured-locally` benchmark rows and recommends a context budget, team lane count, fallback mode, and model route hint without selecting a real model artifact or claiming public benchmark parity. `monitor export` emits the runtime ledger as JSONL or CSV. `monitor prune` is currently dry-run only.

`ontology status`, `ontology seed`, and `ontology inspect` manage the project-local `.rpotato/ontology/graph.jsonl` typed graph store and `.rpotato/ontology/schema.json` contract. Layer A seed records deterministic facts such as indexed files, package manifests, entrypoints, and generated-exclusion rules with source pointers and SHA-256 hashes. `ontology context --query <text>` renders a compact source-pointer-first context view for small-model prompts. `ontology reread <source-pointer>` reopens the authoritative project file and reports the current file hash before any edit decision. `ontology export --format json|jsonl` emits inspection views only; JSON/YAML/RDF/OWL-style exports are not more authoritative than the runtime store. `ontology import --file <path> --dry-run` validates import candidates and blocks confirmed Layer B semantic claims that lack source pointers and source hashes.

Official binary downloads are distributed through GitHub Releases. Starting in v0.28.5, the release workflow builds macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), Linux x86_64 (`x86_64-unknown-linux-gnu`), Linux ARM64 (`aarch64-unknown-linux-gnu`), and Windows x86_64 (`x86_64-pc-windows-msvc`) `rpotato` archives, emits matching basename-only `.sha256` checksum files plus an aggregate checksums file, and runs packaged-binary smoke tests before uploading assets. The Windows job also runs the portable streaming/generation suites and a real fake-sidecar process cancellation lifecycle test natively.

`benchmark validate <fixture.json>` validates project-local benchmark fixture metadata, including runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required tool/source/evidence records, abstention requirement, ontology view, context budget, backend/model artifact identifiers, sampling policy, and raw artifact retention policy. `benchmark record --fixture <fixture.json>` records a metadata-only benchmark run in the append-only ledger and SQLite `benchmark_runs` projection with `claim_state=not-comparable`, no score, a reproducibility manifest, and a redacted local report. `benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]` executes the prompt artifact through the running backend sidecar, records `claim_state=measured-locally`, deterministic 0-3 local product score metadata, `model_run_id`, token/latency/resource summaries, and redacted reproducibility fields without storing raw prompt/source text in SQLite. `benchmark report --format jsonl` exports those redacted benchmark records. Benchmark output still does not claim public benchmark parity.

`model list`, `model manifest`, `model inspect`, `model registry`, and `model download-plan` expose source-backed manifest structure, candidate status, benchmark source ledgers, local registry paths, and pre-download source/license/checksum fields. Qwen and Gemma have pinned source-backed GGUF candidates. `model fetch-candidate <id> --for-evaluation` downloads only to app-managed storage, verifies size and SHA-256, and does not install. `model promote <id> --evidence <file>` requires an exact `backend.chat.completed` provenance record and the hash-pinned canonical `model-adoption-smoke-v1` benchmark/prompt pair before writing local promotion evidence. `model install` revalidates that evidence before registry registration. `model default <id>` selects only a valid registered model, while `model default` shows it; `backend start` may omit `--model` and then fails closed unless the persistent selection, registry, artifact, and promotion evidence all revalidate. The 2026-07-11 strict local comparison selected Gemma because Qwen added an extra line and failed exact-response equality; details are in [docs/model-eval.md](docs/model-eval.md). This is not public benchmark parity or 16 GB validation.

`plugin import` accepts only local Codex/Claude Code-style plugin directories. It snapshots the source into app data, records source manifest and source snapshot SHA-256 hashes in a normalized schema v2 manifest, maps visible capabilities, and reports required and blocked permissions. `plugin validate` and `plugin enable` re-check the imported snapshot hash and mark the plugin `blocked` on drift. Import and enable never grant shell, MCP, hook, background, runtime-setting, remote-connector, sensitive-config, or file-write execution authority by themselves.

`backend doctor` shows managed `llama.cpp` sidecar discovery, environment override path, port, health URL, executable bit, install gate state, and version detection for recorded managed binaries. `backend install-plan` selects a source-backed `llama.cpp` release `b9878` CPU artifact for supported OS/CPU pairs and displays the release URL, archive URL, SHA-256, size, license source, and download path. `backend install` downloads or reuses the cached archive, verifies size and SHA-256, extracts it in staging, places the release payload in the managed backend directory, sets executable permissions on Unix, rolls back failed replacement, writes an install record, and records a ledger event. `backend start [--model <path>] [--ctx-size <tokens>]` starts the selected sidecar with an explicit local model or the revalidated persistent default, records pid/log paths, waits for `/health`, samples CPU/RSS/disk resource status, and kills the child on startup timeout. `backend status` reads the sidecar pid record, health status, and latest sampled resource pressure for running sidecars. `backend stop` requests generation cancellation and waits for its terminal outcome before terminating the sidecar. Env override binaries are not executed by `doctor`; they are executed only by explicit lifecycle commands. `backend verify-archive` verifies a local backend archive SHA-256. `backend health-check` checks `/health` on the selected host and port with a short timeout. `backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]` applies the resource governor and consumes `/v1/chat/completions` SSE. The default display buffers filtered deltas; `--stream` emits complete language-guarded units. The 30-second default timeout, capped at 300 seconds, covers resolution through response reading. `backend cancel` closes the active chat connection, waits for the exact terminal outcome, and leaves the sidecar running. Sent requests are not retried, incomplete final usage remains unknown, split reasoning traces are filtered before display, upstream error details are redacted, and raw prompt/response text is not stored.

Backend CPU/RSS/disk resource sampling is available through `backend start`, `backend status`, `backend chat`, `monitor status`, and the read-only `tui monitor` resource-pressure panel. The first runtime resource governor slice is active for backend chat. `team status` remains a read-only admission preview and now surfaces the latest `team.*` runtime ledger event for the current project. `team admit --lanes <count>` is the enforced team admission gate: it records the decision in the ledger, admits parallel lanes on normal pressure, falls back to one sequential lane on unknown/degraded pressure, and blocks dispatch on critical pressure before any worker launch exists. `team admit` also accepts repeated `--write <path>`, `--write-owner <lane:path>`, and `--command <command>` preflight checks. Policy `ask` or `deny` results block dispatch, and duplicate normalized write ownership across lanes blocks dispatch before worker launch. Policy/ownership blocks write a project-local approval request under `.rpotato/approval-requests/`; `rpotato tui approvals` lists those team requests alongside patch proposals. `team dispatch --lanes <count> --write-owner <lane:path>` rechecks normalized file ownership at dispatch time, blocks cross-lane conflicts, records the result in the ledger/SQLite projection, and can record failed-worker continuation with `--failed-lane <lane> --failure <reason>` without starting workers or advancing team stages. `team governor --lanes <count> --context-tokens <tokens>` records the first context/model governor preflight: it reports admitted lanes, clamps requested context against the configured budget and current resource pressure, and emits local model-tier route hints (`keep`, `downgrade`, `escalate`, or `defer`) without claiming real model capability or selecting model artifacts.

## Documentation

English documentation is the default repository surface. Korean translations are available separately:

- [Korean README](README.ko.md)
- [Korean documentation directory](docs/ko/)

Start with:

- [PLAN.md](PLAN.md)
- [ROADMAP.md](ROADMAP.md)
- [docs/release-train.md](docs/release-train.md)
- [docs/runtime-architecture.md](docs/runtime-architecture.md)
- [docs/ontology-runtime.md](docs/ontology-runtime.md)
- [docs/architecture.md](docs/architecture.md)
- [docs/mvp.md](docs/mvp.md)

## Governance

This is a public open-source repository, but external code contributions and external PRs are not currently accepted. Bug reports, usability feedback, security reports, and model/license evidence may be accepted. See [GOVERNANCE.md](GOVERNANCE.md) and [SECURITY.md](SECURITY.md).
