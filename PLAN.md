# rolling-potato Plan

## Name

- Project name: `rolling-potato`
- CLI command: `rpotato`
- Tagline: `Local coding agents for potato PCs.`
- Korean positioning: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

## Intent

`rolling-potato` is a local-first CLI coding agent runtime for small models.

The goal is not to clone Claude Code or Codex with a weaker model. The goal is to build a runtime that makes small local models useful by limiting their failure modes.

Core thesis:

> Small models need a small-model runtime, not just a smaller prompt.

Claude Code and Codex assume frontier-class model capability. `rolling-potato` assumes the opposite: the model is useful but fragile, so the runtime must manage context, actions, validation, retries, and user-facing language.

## Target Users

- Korean-speaking users first
- users who find cloud coding-agent subscriptions expensive
- users with low-end or mid-range laptops
- users who want local/private execution
- non-expert or semi-technical users who still need coding help

Initial hardware target:

- 16 GB RAM laptop
- macOS and Windows first
- Linux later or contributor-driven

## Product Shape

Primary interface:

- CLI, similar in spirit to Claude Code / Codex

Initial command sketch:

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato model list
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato cache status
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

The CLI should feel lightweight and direct. It should not require users to understand local LLM tooling before they can start.

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
- Tauri/Electron, because the first product is CLI, not GUI

## Managed Backend Distribution

Users should not have to install `llama.cpp` manually for the MVP path.

Expected backend flow:

1. `rpotato init` checks the host OS, architecture, RAM, disk, and existing config.
2. `rpotato` resolves a source-verified backend release for the current platform.
3. The user approves any network download.
4. The backend archive is downloaded with resume support.
5. The archive checksum is verified before extraction.
6. The extracted backend binary is stored under the `rpotato` app data root.
7. `rpotato backend doctor` verifies binary path, executable bit, version, port readiness, and health check behavior.
8. `rpotato run` starts or reuses the sidecar as a child process, records PID/port/log paths, and shuts it down when the owning session ends unless reuse is enabled.

The sidecar is "container-like" in ownership but not Docker-based. It is an isolated, CLI-managed child process with explicit paths, logs, port, health check, and cleanup. Docker is not the MVP default because it adds a heavy external prerequisite for low-end macOS/Windows users.

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
- exact upstream model, GGUF artifact, license, checksum, and runtime fit are unverified
- do not describe Korean/code/agent quality, multimodal support, or 16 GB suitability as fact until source-backed evaluation is complete

Comparison candidate:

- `Gemma 4 E4B`

Status:

- comparison candidate only
- license, artifact, multimodal support, and runtime fit are unverified
- useful only after source-backed artifact selection and benchmark design

Not the default:

- `Qwen3.5-9B`, because larger local models may increase pressure on context, verification, and runtime overhead. Exact viability is unverified and needs measurement.

## Model And Runtime Download Flow

Model weights should not be bundled into the initial CLI installer.

Expected flow:

1. User installs `rpotato`.
2. User runs `rpotato init` or `rpotato model install`.
3. CLI checks OS, architecture, RAM, and available disk.
4. CLI verifies or installs the managed backend binary.
5. CLI recommends a source-verified model candidate only after manifest validation.
6. User explicitly agrees to download.
7. CLI downloads the model with resume support.
8. CLI verifies hash.
9. CLI registers the model in local config.
10. CLI starts or reuses the local runtime.

Model metadata should live in a manifest:

```json
{
  "id": "qwen3.5-4b-q4-k-m",
  "displayName": "Qwen3.5 4B",
  "format": "gguf",
  "backend": "llama.cpp",
  "recommendedRamGb": 16,
  "license": "TODO",
  "sha256": "TODO",
  "url": "TODO"
}
```

## Small-Model Runtime Responsibilities

The runtime should own:

- model install/cache management
- backend binary install/cache management
- model process lifecycle
- prompt compilation per model
- context packing
- repo/file indexing
- tool permission policy
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
  cache/

project root/
  .rpotato/           # optional project-local state, indexes, evidence
```

Platform paths are decided during Phase 1, but the boundary should stay stable:

- `backends/` and the `rpotato` launcher are program/runtime assets.
- `models/`, `downloads/`, `manifests/`, generated context indexes, and logs are cache/data assets.
- project-local `.rpotato/` is user project state and must not be removed by global uninstall unless the user explicitly asks for project cleanup from that project.

## Uninstall And Cache Policy

Uninstall must be executable from the CLI and must show a dry-run summary before deleting anything.

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

Default to sequential agents, not parallel agents.

Initial roles:

- planner: create a short structured plan
- executor: propose a small action or patch
- verifier: inspect command/test/log output
- reporter: produce the final Korean-only user response

Avoid by default:

- parallel decoding
- loading the model multiple times
- large context dumps
- unbounded shell access
- long free-form reasoning output

## Korean-Only Requirement

User-facing output must be Korean-only unless code or exact file contents are explicitly required.

Runtime guard:

- detect English, Chinese, and Japanese leakage
- reject mixed-language final answers
- regenerate once with stricter instruction
- fail closed with a Korean-only error if still invalid
- keep code blocks separate from natural-language output

## CLI Safety Model

Default behavior should be conservative:

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

- Rust: preferred for single-binary CLI, process control, packaging, and cross-platform reliability
- TypeScript/Node: faster prototype, but weaker for self-contained distribution

Current lean:

- Rust core CLI
- managed `llama.cpp` sidecar
- adapter boundary for future backends

## MVP Definition

The first useful version should:

1. install and run as `rpotato`
2. install or verify a managed `llama.cpp` backend after user consent when download is needed
3. download one recommended GGUF model after user consent
4. start local inference backend
5. chat in Korean
6. inspect a local repo
7. propose a small patch
8. show the diff before applying
9. run a verification command when approved
10. produce a Korean-only final report
11. uninstall managed runtime assets through CLI with keep-cache and purge-cache paths

## Open Questions

- Rust first, or TypeScript prototype first?
- Which exact Qwen3.5-4B GGUF artifact should be trusted?
- Which `llama.cpp` release artifact and checksum source should be trusted per platform?
- How should self-delete work on Windows package-manager installs?
- Should image/screenshot understanding be MVP or later?
- How strict should command approval be?
- Should `rpotato` support non-code general automation later?
- What is the first benchmark suite for Korean/code/tool reliability?

## Current Documentation

This initial plan has been split into:

1. `README.md` positioning draft
2. `docs/architecture.md`
3. `docs/model-eval.md`
4. `docs/mvp.md`

Open-source operating docs have also been added:

1. `LICENSE`
2. `GOVERNANCE.md`
3. `MAINTAINERS.md`
4. `SECURITY.md`
5. `PRIVACY.md`
6. `ROADMAP.md`
7. `docs/development.md`
8. `docs/release.md`
9. `docs/model-manifest.md`
10. `docs/model-licenses.md`
11. `docs/model-source-policy.md`
12. `docs/backend-adapters.md`
13. `docs/command-policy.md`
14. `docs/korean-output-guard.md`
15. `docs/threat-model.md`
16. `docs/benchmarks.md`

Project-local automation and contribution policy is recorded in `AGENTS.md`: external code PRs are not accepted, safe verified units should be committed and pushed automatically, and commit messages use Conventional Commits in the form `type(scope): title`.

Model-related claims require explicit sources. Model names, licenses, artifact URLs, checksums, RAM requirements, backend compatibility, multimodal support, and quality claims must follow `docs/model-source-policy.md`.

Next implementation-oriented decisions:

1. choose the exact trusted `Qwen3.5-4B` GGUF artifact
2. define the initial model manifest format on disk
3. scaffold the Rust CLI
4. implement `rpotato doctor` before agent behavior
5. build the first fixture benchmark for Korean/code/tool reliability
