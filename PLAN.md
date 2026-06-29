# rolling-potato Plan

## Name

- Project name: `rolling-potato`
- CLI command: `rpotato`
- Tagline: `Local coding agents for potato PCs.`
- Korean positioning: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

## Intent

`rolling-potato` is a local-first coding agent runtime for small models. Its first user surface is the `rpotato` CLI.

The goal is not to clone Claude Code or Codex with a weaker model. The goal is to build a runtime that makes small local models useful by limiting their failure modes.

Core thesis:

> Small models need a small-model runtime, not just a smaller prompt.

Claude Code, Codex, and Gaja Code style tools expose a CLI-like agent experience, but the product is the runtime behind that surface. `rolling-potato` assumes the model is useful but fragile, so the runtime core must manage context, ontology, hooks, skills, subagents, team execution, actions, validation, retries, and user-facing language.

## Target Users

- Korean-speaking users first
- users who find cloud coding-agent subscriptions expensive
- users with low-end or mid-range laptops
- users who want local/private execution
- non-expert or semi-technical users who still need coding help

Initial hardware target:

- 16 GB RAM laptop
- macOS and Windows first
- Linux later, maintainer-led or after governance policy changes

## Product Shape

Primary surface:

- CLI, similar in spirit to Claude Code / Codex / Gaja Code
- TUI, required for replacement-level interactive use

Product body:

- runtime core for state, policy, ontology, context, agent loop, evidence, and stop gates
- backend/model layer for local inference
- observability layer for model/token/resource monitoring
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
rpotato tui
rpotato skill list
rpotato skill run fix-test
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin import --from codex ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato plugin enable imported.example-plugin
rpotato team status
rpotato model list
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

The CLI should feel lightweight and direct. It is not the product boundary; it is the first way users drive the runtime. It should not require users to understand local LLM tooling before they can start.

## Runtime Direction

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

## Managed Backend Distribution

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

## Initial Model Direction

Priority evaluation candidate:

- `Qwen3.5-4B` quantized GGUF

Status:

- user-directed candidate, not a confirmed default
- exact GGUF artifact, artifact provider terms, checksum, and runtime fit are unverified
- do not describe Korean/code/agent quality, multimodal support, or 16 GB suitability as fact until source-backed evaluation is complete

Comparison candidate:

- `Gemma 4 E4B`

Status:

- comparison candidate only
- artifact, artifact provider terms, multimodal support, and runtime fit are unverified
- useful only after source-backed artifact selection and benchmark design

Not the default:

- `Qwen3.5-9B`, because larger local models may increase pressure on context, verification, and runtime overhead. Exact viability is unverified and needs measurement.

## Model And Runtime Download Flow

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

## Small-Model Runtime Responsibilities

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

## Storage Layout

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

## Observability And Monitoring

Model monitoring is a required runtime capability, not a later analytics add-on.

Default decision:

- Use SQLite as the local query/index/reporting store.
- Keep append-only ledger/JSONL as the audit trail and crash-recovery source.
- Store token, latency, backend, guard, tool, evidence, and stop-gate metrics by session/workflow/model.
- Do not store raw prompt, source code, or credential-bearing command output by default.
- Expose monitoring through `rpotato monitor ...`, `doctor`, benchmark reports, and TUI views.

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

## Uninstall And Cache Policy

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
- If the CLI was installed by a package manager, `rpotato uninstall` should clean app-owned data and print the exact package-manager removal command instead of pretending it can always remove the package manager's binary.
- On platforms where deleting the currently running binary is unsafe or impossible, `rpotato uninstall` should write a small post-exit cleanup script or print the final manual command in Korean.
- Every delete path must support `--dry-run`, path listing, and Korean confirmation text before execution.

## Agent Strategy

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

## Korean-Only Requirement

User-facing output must be Korean-only unless code or exact file contents are explicitly required.

Runtime guard:

- detect English, Chinese, and Japanese leakage
- reject mixed-language final answers
- regenerate once with stricter instruction
- fail closed with a Korean-only error if still invalid
- keep code blocks separate from natural-language output

## CLI Safety Model

The CLI surface displays and asks. The runtime core decides and enforces. Default behavior should be conservative:

- read files freely inside the selected project
- require confirmation before writing files
- require confirmation before running commands with side effects
- show diffs before applying changes
- keep an operation log
- provide `doctor` for local runtime/model diagnostics

This can be relaxed later with trust modes.

## Publishing Direction

Initial publishing:

- GitHub repo
- GitHub Releases for binaries
- model manifest in repo or release asset

Likely package channels:

- Homebrew for macOS/Linux
- Scoop or winget for Windows
- npm wrapper only if JavaScript ecosystem adoption matters

Implementation language candidates:

- Rust: preferred for single-binary distribution, process control, packaging, and cross-platform reliability
- TypeScript/Node: faster prototype, but weaker for self-contained distribution

Current lean:

- Rust runtime core with CLI surface
- terminal TUI as required product surface
- managed `llama.cpp` sidecar
- adapter boundary for future backends

## MVP Definition

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

## Open Questions

- Rust first, or TypeScript prototype first?
- Which exact Qwen3.5-4B GGUF artifact should be trusted?
- Which `llama.cpp` release artifact and checksum source should be trusted per platform?
- How should self-delete work on Windows package-manager installs?
- Should image/screenshot understanding be MVP or later?
- How strict should command approval be?
- What hooks are enabled by default?
- What skills ship first?
- Which compatibility target comes first: Codex plugins or Claude Code plugins?
- Which foreign plugin capabilities are allowed before marketplace support?
- What subagent concurrency limit is safe on 16 GB RAM?
- What team pipeline is required for replacement-level workflows?
- Which Rust TUI framework should be used?
- Which Rust SQLite crate should be used?
- What is the default monitoring retention period?
- Should `rpotato` support non-code general automation later?
- What is the first benchmark suite for Korean/code/tool reliability?

## Current Documentation

Core design docs:

1. `README.md` positioning draft
2. `DESIGN.md`
3. `docs/architecture.md`
4. `docs/model-eval.md`
5. `docs/mvp.md`
6. `docs/runtime-architecture.md`
7. `docs/glossary.md`
8. `docs/ontology-runtime.md`
9. `docs/observability.md`
10. `docs/hooks.md`
11. `docs/skills.md`
12. `docs/subagents.md`
13. `docs/team-runtime.md`
14. `docs/tui.md`
15. `docs/plugin-adapters.md`

Open-source operating docs:

1. `LICENSE`
2. `GOVERNANCE.md`
3. `MAINTAINERS.md`
4. `SECURITY.md`
5. `PRIVACY.md`
6. `ROADMAP.md`
7. `docs/development.md`
8. `docs/release.md`

Runtime policy and validation docs:

1. `docs/model-manifest.md`
2. `docs/model-licenses.md`
3. `docs/model-source-policy.md`
4. `docs/backend-adapters.md`
5. `docs/command-policy.md`
6. `docs/korean-output-guard.md`
7. `docs/threat-model.md`
8. `docs/benchmarks.md`
9. `docs/observability.md`
10. `docs/hooks.md`
11. `docs/skills.md`
12. `docs/subagents.md`
13. `docs/team-runtime.md`
14. `docs/tui.md`
15. `docs/plugin-adapters.md`

Project-local automation and contribution policy is recorded in `AGENTS.md`: external code PRs are not accepted, safe verified units should be committed and pushed automatically, and commit messages use Conventional Commits in the form `type(scope): title`.

Model-related claims require explicit sources. Model names, licenses, artifact URLs, checksums, RAM requirements, backend compatibility, multimodal support, and quality claims must follow `docs/model-source-policy.md`.

Next implementation-oriented decisions:

1. choose the exact trusted `Qwen3.5-4B` GGUF artifact
2. define the initial model manifest format on disk
3. separate runtime core modules from the CLI surface
4. define normalized plugin manifest and inspect/validate output before plugin execution
5. implement `rpotato doctor` before agent behavior
6. build the first fixture benchmark for Korean/code/tool reliability
