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
rpotato doctor
rpotato config
```

The CLI should feel lightweight and direct. It should not require users to understand local LLM tooling before they can start.

## Runtime Direction

Default runtime direction:

- `llama.cpp` backend
- GGUF model format
- bundled or managed runtime binary
- local HTTP/server mode or sidecar process

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

## Model Download Flow

Model weights should not be bundled into the initial CLI installer.

Expected flow:

1. User installs `rpotato`.
2. User runs `rpotato init` or `rpotato model install`.
3. CLI checks OS, architecture, RAM, and available disk.
4. CLI recommends a model.
5. User explicitly agrees to download.
6. CLI downloads the model with resume support.
7. CLI verifies hash.
8. CLI registers the model in local config.
9. CLI starts or reuses the local runtime.

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
- llama.cpp sidecar
- adapter boundary for future backends

## MVP Definition

The first useful version should:

1. install and run as `rpotato`
2. download one recommended GGUF model after user consent
3. start local inference backend
4. chat in Korean
5. inspect a local repo
6. propose a small patch
7. show the diff before applying
8. run a verification command when approved
9. produce a Korean-only final report

## Open Questions

- Rust first, or TypeScript prototype first?
- Use `llama-server` sidecar or native llama.cpp binding?
- Which exact Qwen3.5-4B GGUF artifact should be trusted?
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
