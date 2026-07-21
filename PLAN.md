# rolling-potato Plan

## 1. Product Definition

### Name

- Project name: `rolling-potato`
- CLI command: `rpotato`
- Tagline: `Local coding agents for potato PCs.`
- Korean positioning: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

### Intent

`rolling-potato` is a local-first coding agent runtime for small models. Its first user surface is the `rpotato` CLI.

The goal is not to clone Claude Code or Codex with a weaker model. The goal is to build a runtime that makes small local models useful by limiting their failure modes.

Core thesis:

> Small models need a small-model runtime, not just a smaller prompt.

Claude Code, Codex, and Gaja Code style tools expose a CLI-like agent experience, but the product is the runtime behind that surface. `rolling-potato` assumes the model is useful but fragile, so the runtime core must manage context, ontology, hooks, skills, subagents, team execution, actions, validation, retries, and user-facing language.

### Target Users

- Korean-speaking users first
- users who find cloud coding-agent subscriptions expensive
- users with low-end or mid-range laptops
- users who want local/private execution
- non-expert or semi-technical users who still need coding help

Current hardware and release baseline:

- 16 GB RAM class laptops remain the product target; exact model fit still
  requires local evidence.
- Official release artifacts cover macOS arm64/x64, Linux arm64/x64, and
  Windows x64.

### Product Shape

Primary surface:

- CLI, similar in spirit to Claude Code / Codex / Gaja Code
- TUI, required for replacement-level interactive use

Product body:

- runtime core for state, policy, ontology, context, agent loop, evidence, and stop gates
- backend/model layer for local inference
- observability layer for model/token/resource monitoring
- model knowledge base for source-backed LLM evidence and repeated runtime observations
- state lifecycle contract for ledger, SQLite, ontology, model knowledge, plugin registry, and evidence replay
- hook system for lifecycle control points
- skill system for reusable workflows
- subagent and team runtime for bounded multi-agent work
- plugin adapter layer for importing Claude Code/Codex-style plugin packages into runtime-owned capabilities
- CLI/TUI surfaces for user input, streaming display, approval prompts, diffs, status, and final reports

Initial command sketch:

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato intent classify "테스트 실패 고쳐줘"
rpotato intent routes
rpotato tui
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
rpotato policy check-command "cargo test"
rpotato hooks list
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin import --from codex ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato plugin enable imported.example-plugin
rpotato team status
rpotato team admit --lanes 2
rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
rpotato model list
rpotato model manifest
rpotato model inspect qwen3.5-4b
rpotato model registry
rpotato model knowledge
rpotato model knowledge inspect qwen3.5-4b
rpotato model download-plan qwen3.5-4b
rpotato model verify-file ./model.gguf --sha256 <64-hex>
rpotato model cleanup-failed qwen3.5-4b --dry-run
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato backend install-plan
rpotato backend verify-archive ./llama.cpp.zip --sha256 <64-hex>
rpotato backend health-check
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor export --format html > rpotato-monitor.html
rpotato monitor prune --before 30d --dry-run
rpotato benchmark validate benchmarks/fixtures/sample.json
rpotato benchmark record --fixture benchmarks/fixtures/sample.json
rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32
rpotato benchmark report --format jsonl
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

The CLI should feel lightweight and direct. It is not the product boundary; it is the first way users drive the runtime. It should not require users to understand local LLM tooling before they can start.

Plugin adapter commands use local plugin directory paths only. `rpotato` does not integrate with external plugin marketplaces, registries, catalogs, or package mirrors.

## 2. Runtime and Model Foundation

### Runtime Direction

Default runtime direction:

- `llama.cpp` backend
- GGUF model format
- managed `llama-server` / `llama.cpp` runtime binary
- local HTTP/server sidecar process owned by `rpotato`

Why:

- works across macOS, Windows, and Linux
- fits quantized 4B models
- avoids Mac-only dependency on MLX
- avoids requiring WSL/CUDA/PyTorch like vLLM often does
- easier to package than a full desktop app stack

Optional adapters later:

- LM Studio adapter for users who already use it
- Ollama adapter for users who already have models installed
- vLLM/SGLang adapter for server/GPU mode

Excluded as default:

- MLX, because it is Apple Silicon specific
- vLLM, because it is better as a server/GPU backend than a default low-end local runtime
- Tauri/Electron, because the required interactive surface is terminal TUI before GUI

### Managed Backend Distribution

Users should not have to install `llama.cpp` manually for the MVP path.

Expected backend flow:

1. `rpotato init` sends an init request to the runtime core.
2. The runtime core checks the host OS, architecture, RAM, disk, and existing config.
3. The runtime core resolves a source-verified backend release for the current platform.
4. The CLI surface asks the user to approve any network download.
5. The runtime core downloads the backend archive with resume support.
6. The runtime core verifies the archive checksum before extraction.
7. The extracted backend binary is stored under the `rpotato` app data root.
8. `rpotato backend doctor` renders runtime diagnostics for binary path, executable bit, version, port readiness, and health check behavior.
9. `rpotato run` asks the runtime core to start or reuse the sidecar as a child process, record PID/port/log paths, and shut it down when the owning session ends unless reuse is enabled.

The sidecar is "container-like" in ownership but not Docker-based. It is an isolated, runtime-managed child process with explicit paths, logs, port, health check, and cleanup. Docker is not the MVP default because it adds a heavy external prerequisite for low-end macOS/Windows users.

Manual backend override remains possible later through config:

```sh
rpotato config set backend.llama_cpp.path /path/to/llama-server
```

An overridden backend is user-owned. `rpotato uninstall` must not delete it.

### Initial Model Direction

Priority evaluation candidate:

- `Qwen3.5-4B-Q4_K_M.gguf` from `unsloth/Qwen3.5-4B-GGUF`

Status:

- user-directed candidate, not a confirmed default
- artifact URL, provider page, LFS SHA-256, and file size are source-recorded as `unverified`
- local `llama.cpp b9878` smoke, RAM fit, text-only mmproj need, and benchmark fit are still unverified
- do not describe Korean/code/agent quality, multimodal support, or 16 GB suitability as fact until source-backed evaluation is complete

Comparison candidate:

- `gemma-4-E4B_q4_0-it.gguf` from `google/gemma-4-E4B-it-qat-q4_0-gguf`

Status:

- comparison candidate only
- artifact URL, provider page, LFS SHA-256, and file size are source-recorded as `unverified`
- multimodal support, text-only mmproj need, runtime fit, and benchmark fit are still unverified
- useful only after local runtime validation and benchmark execution

Not the default:

- `Qwen3.5-9B`, because larger local models may increase pressure on context, verification, and runtime overhead. Exact viability is unverified and needs measurement.

### Model And Runtime Download Flow

Model weights should not be bundled into the initial `rpotato` release artifact.

Expected flow:

1. User installs `rpotato`.
2. User runs `rpotato init` or `rpotato model install`.
3. CLI surface forwards the request to the runtime core.
4. Runtime core checks OS, architecture, RAM, and available disk.
5. Runtime core verifies or installs the managed backend binary.
6. Runtime core recommends a source-verified model candidate only after manifest validation.
7. CLI surface asks the user to explicitly approve download.
8. Runtime core downloads the model with resume support.
9. Runtime core verifies hash.
10. Runtime core registers the model in local config.
11. Runtime core starts or reuses the local inference backend.

Model metadata should live in a manifest:

```json
{
  "id": "qwen3.5-4b-q4-k-m",
  "displayName": "Qwen3.5 4B",
  "format": "gguf",
  "backend": "llama.cpp",
  "recommendedRamGb": null,
  "license": "TODO",
  "sha256": "TODO",
  "url": "TODO"
}
```

This is a schema sketch. `null` and `TODO` values are placeholders, not product facts.

### Small-Model Runtime Responsibilities

The runtime should own:

- model install/cache management
- backend binary install/cache management
- model process lifecycle
- session lifecycle and state transitions
- hook lifecycle
- skill invocation and state
- foreign plugin import and normalized capability validation
- prompt compilation per model
- token usage accounting per model
- model knowledge/evidence indexing
- ontology and context lifecycle
- context packing
- repo/file indexing
- model/runtime monitoring
- tool permission policy
- subagent lifecycle
- team coordination
- structured action schemas
- constrained output where possible
- retry policy
- diff generation and validation
- command/test/log feedback
- final Korean-only response validation

## 3. State, Evidence, and Local Data

### Storage Layout

The implementation should keep install-time assets, cache, and project state separate so uninstall behavior is predictable.

Initial logical roots:

```text
rpotato app data root/
  config/
  backends/           # managed llama.cpp binaries and metadata
  models/             # GGUF model artifacts
  downloads/          # resumable partial downloads
  manifests/          # model/backend manifests
  logs/
  state/
    observability.sqlite
    runtime-ledger.jsonl
    transcripts/<project-id>/<session-id>/*.json
  plugins/
    imported/
    data/
  cache/

project root/
  .rpotato/           # optional project-local state, indexes, evidence
```

Platform paths are decided during Phase 1, but the boundary should stay stable:

- `backends/` and the `rpotato` launcher are program/runtime assets.
- `models/`, `downloads/`, `manifests/`, generated context indexes, SQLite monitoring store, and logs are cache/data assets.
- project-local `.rpotato/` is user project state and must not be removed by global uninstall unless the user explicitly asks for project cleanup from that project.

### Observability And Monitoring

Model monitoring is a required runtime capability, not a later analytics add-on.

Default decision:

- Use SQLite as the local query/index/reporting store.
- Keep append-only ledger/JSONL as the audit trail and crash-recovery source.
- Store token, latency, backend, guard, tool, evidence, stop-gate, and rebuildable transcript projections by session/workflow/model.
- Persist local user turns plus visible/normalized model, tool, and evidence turns for resume. Do not persist the complete backend prompt, hidden/raw model response, raw source body, or credential-bearing command output.
- Expose monitoring through `rpotato monitor ...`, `doctor`, benchmark reports, and TUI views.

### Model Knowledge Base

The LLM wiki is introduced as a model knowledge base: an evidence index over
manifest records, benchmark results, observability metrics, and source-backed
claims.

It is useful for automatic maintenance, but only with gates.

- Agents may automatically add `observed` or `candidate` notes from repeated
  runtime evidence.
- Frequency can raise priority and trigger investigation.
- Frequency alone cannot confirm model quality, license, backend compatibility,
  RAM fit, or default-model status.
- `measured-locally` requires benchmark/run ids, artifact hash, environment,
  prompt/runtime version, and scoring evidence.
- Source/license/artifact confirmation stays under the model manifest and
  model source policy.
- Raw prompt and raw source text are not stored in the model knowledge base by
  default.

Required model metrics:

- prompt tokens
- completion tokens
- total tokens
- context tokens used and dropped
- ontology/tool-summary token budget
- first token latency
- total latency
- tokens per second
- backend startup time
- peak memory
- retry/regeneration count
- Korean guard rejection count
- stop gate pass/fail

SQLite is appropriate because the runtime needs cross-session queries such as model-level token totals, failure rates, latency percentiles, and benchmark-vs-real-run comparisons. The append-only ledger remains the event source; SQLite is the projection for fast local queries.

`benchmark run` is the first executable benchmark slice. It reads a
project-local prompt artifact, calls the active backend sidecar, records a local
`measured-locally` 0-3 product score, links the benchmark row to `model_run_id`,
and stores token/latency/resource summaries plus redacted reproducibility
metadata. It does not store raw prompt/source text in SQLite and does not claim
public benchmark parity.

### Uninstall And Cache Policy

Uninstall must be exposed through the CLI surface and must show a dry-run summary before deleting anything.

Commands:

```sh
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato uninstall --dry-run --purge-cache
rpotato cache status
rpotato cache clean --models
rpotato cache clean --downloads
```

Behavior:

- `--keep-cache`: remove `rpotato`-managed program/runtime assets and launcher registrations, but keep downloaded models, partial downloads, manifests, logs, and project-local `.rpotato/` state.
- `--purge-cache`: remove program/runtime assets plus app-level caches such as models, downloads, backend archives, manifests, logs, and generated indexes.
- `--purge-cache` still does not delete source repositories or project files. Project-local cleanup requires a separate project-scoped command such as `rpotato project clean --dry-run`.
- Because the supported install path is a downloaded GitHub Release archive,
  `rpotato uninstall` should clean app-owned data and print the final manual
  executable-removal command instead of pretending it can always delete the
  currently running binary.
- On platforms where deleting the currently running binary is unsafe or impossible, `rpotato uninstall` should write a small post-exit cleanup script or print the final manual command in Korean.
- Every delete path must support `--dry-run`, path listing, and Korean confirmation text before execution.

## 4. Agent Behavior and Safety

### Agent Strategy

Default to sequential agents for small tasks. Support subagents and team execution for tasks that materially benefit from parallel or staged work.

Initial roles:

- planner: create a short structured plan
- executor: propose a small action or patch
- verifier: inspect command/test/log output
- reporter: produce the final Korean-only user response

Avoid by default:

- unbounded parallel decoding
- loading the model multiple times
- large context dumps
- unbounded shell access
- long free-form reasoning output

Required advanced runtime capabilities:

- lifecycle hooks
- reusable skills
- bounded subagents
- team orchestration
- TUI surface

### Korean-Only Requirement

User-facing output must be Korean-only unless code or exact file contents are explicitly required.

Runtime guard:

- detect English, Chinese, and Japanese leakage
- reject mixed-language final answers
- regenerate once with stricter instruction
- fail closed with a Korean-only error if still invalid
- keep code blocks separate from natural-language output

### CLI Safety Model

The CLI surface displays and asks. The runtime core decides and enforces. Default behavior should be conservative:

- read files freely inside the selected project
- require confirmation before writing files
- require confirmation before running commands with side effects
- show diffs before applying changes
- keep an operation log
- provide `doctor` for local runtime/model diagnostics

This can be relaxed later with trust modes.

## 5. Delivery and Open Decisions

### Publishing Direction

Initial publishing:

- GitHub repo
- GitHub Releases as the only binary distribution channel, with archives for
  all supported targets plus per-asset `.sha256` files and an aggregate
  checksums file
- model manifest in repo or release asset

Homebrew, Scoop, winget, npm wrappers, and other external package repositories
are intentionally outside the distribution plan.

Implementation language candidates:

- Rust: preferred for single-binary distribution, process control, packaging, and cross-platform reliability
- TypeScript/Node: faster prototype, but weaker for self-contained distribution

Current lean:

- Rust runtime core with CLI surface
- terminal TUI as required product surface
- managed `llama.cpp` sidecar
- adapter boundary for future backends

### MVP Definition

The first useful version should:

1. install and run through the `rpotato` CLI surface
2. install or verify a managed `llama.cpp` backend after user consent when download is needed
3. download one recommended GGUF model after user consent
4. start local inference backend
5. chat in Korean
6. inspect a local repo
7. propose a small patch
8. show the diff before applying
9. run a verification command when approved
10. produce a Korean-only final report
11. uninstall managed runtime assets through the runtime with keep-cache and purge-cache paths exposed by CLI

Replacement-level beta should additionally:

1. expose a TUI surface
2. run skills with hook-attached policy and evidence gates
3. support bounded subagents
4. support team execution with runtime-owned merge and stop gates
5. import Claude Code/Codex-style plugin packages only through adapter validation and runtime policy gates
6. show approvals, diff, tool output, subagent/team status, plugin permission review, and evidence in the TUI

### Resolved Decisions

- The runtime core and CLI are implemented in Rust.
- SQLite projection ownership uses `rusqlite`; canonical ledgers remain the
  authority.
- Managed backend artifacts are source-pinned to `llama.cpp b9982`.
- Command approval, native hooks, built-in skills, bounded subagents, team
  execution, and local plugin adapters have runtime-owned policy boundaries.
- Plugin import is local-directory only. Remote marketplaces, registries, and
  catalogs are outside the supported boundary.
- Monitoring uses CLI/TUI surfaces plus an optional self-contained local HTML
  export backed by the same SQLite/ledger data.

### Open Questions

- Which source-backed candidate can become a supported default after exact
  `b9982`, 16 GB RAM-fit, mmproj, quality, and benchmark evidence?
- Should image or screenshot understanding enter a future version?
- Should a richer TUI adopt a framework beyond the current std-only line
  controller?
- What should the default monitoring retention period be?
- Which measured subagent/team lane and context budgets are safe on 16 GB
  machines?
- Should `rpotato` support non-code general automation in a future version?

### Current Documentation

The chaptered [documentation index](docs/README.md) is the navigation source of
truth. The [current-capabilities guide](docs/current-capabilities.md) maps
implemented surfaces and known boundaries without duplicating them here.

Project-local automation and contribution policy is recorded in
[AGENTS.md](AGENTS.md). Model-related claims require explicit sources and must
follow [docs/model-source-policy.md](docs/model-source-policy.md).

`v0.43.0` is the concrete in-development version for the guided default TUI and
bounded small-model context compaction. Add another concrete row to
[ROADMAP.md](ROADMAP.md) before turning a different open question into
implementation work.
