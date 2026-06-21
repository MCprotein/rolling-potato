# rolling-potato Handoff

## Context

This handoff is for continuing the `rolling-potato` project in a new Codex session.

The current Codex session started inside the `homeserver` workspace, but the new project lives outside that workspace:

```text
/Users/sys/Desktop/codes/rolling-potato
```

Open the next session from that directory, not from `/Users/sys/Desktop/homeserver`.

## Current State

Created:

```text
/Users/sys/Desktop/codes/rolling-potato/PLAN.md
/Users/sys/Desktop/codes/rolling-potato/README.md
/Users/sys/Desktop/codes/rolling-potato/docs/architecture.md
/Users/sys/Desktop/codes/rolling-potato/docs/model-eval.md
/Users/sys/Desktop/codes/rolling-potato/docs/mvp.md
```

The plan document contains the initial product direction, runtime assumptions, model direction, publishing direction, MVP definition, and open questions.

The README and docs split that plan into:

- Korean-first product positioning
- Rust CLI and `llama.cpp` sidecar architecture
- MVP acceptance criteria
- Korean/code/tool model evaluation draft

Open-source operating docs have also been added:

- `AGENTS.md`
- `LICENSE`
- `GOVERNANCE.md`
- `MAINTAINERS.md`
- `SECURITY.md`
- `PRIVACY.md`
- `ROADMAP.md`
- `.github/ISSUE_TEMPLATE/`
- `docs/development.md`
- `docs/release.md`
- `docs/model-manifest.md`
- `docs/model-licenses.md`
- `docs/backend-adapters.md`
- `docs/command-policy.md`
- `docs/korean-output-guard.md`
- `docs/threat-model.md`
- `docs/benchmarks.md`

External code contributions and external PRs are not accepted for now. This is now recorded in `AGENTS.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, README, and the GitHub issue templates.

Persistent automation instruction:

- Continue making safe, verified changes without asking for repeated confirmation.
- Commit and push meaningful units automatically to `origin main`.
- Use Conventional Commit messages in the form `type(scope): title`, for example `docs(governance): add open source operating policy`.

An earlier temporary directory also exists:

```text
/Users/sys/Desktop/codes/smallcode-runtime
```

It was an abandoned placeholder from before the name was finalized. It was not deleted.

## Product Name

- Project name: `rolling-potato`
- CLI command: `rpotato`
- Tagline: `Local coding agents for potato PCs.`
- Korean positioning: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

Reasoning:

- `potato PC` is an English low-spec computer meme.
- `rolling` feels active and forward-moving.
- `spinning-potato` was rejected because it can imply being stuck, loading forever, or just spinning in place.
- `rpotato` was chosen as the CLI command because it connects to `rolling-potato` while being shorter and less collision-prone than `rolling-potato`.

## Core Intent

Build a local-first CLI coding agent runtime for small local models.

This is not just a prompt harness. The user specifically corrected the framing:

> It should be runtime plus harness integration. Small models probably need their own runtime. Claude Code is designed for large models.

Accepted thesis:

> Small models need a small-model runtime, not just a smaller prompt.

The product should make small models reliable by narrowing choices, managing context, validating actions, controlling retries, and enforcing Korean-only output.

## Target Users

- Korean-speaking users first
- users who find Claude Code / Codex subscriptions expensive
- users with low-end or mid-range laptops
- users who want local/private execution
- non-expert or semi-technical users

Initial machine target:

- 16 GB RAM laptop
- macOS and Windows first
- Linux later or contributor-driven

## Runtime Direction

Default runtime direction:

- `llama.cpp`
- GGUF model format
- managed sidecar process or later native integration
- cross-platform CLI distribution

Rejected as default:

- MLX: too Mac-specific
- vLLM: better as server/GPU backend, not low-end local default
- Tauri/Electron: GUI app stacks; user wants Claude Code-like CLI
- Ollama as core runtime: convenient, but too heavy/opaque for a tight integrated runtime

Adapters to consider later:

- LM Studio: useful for demo, marketing, and users who already use it
- Ollama: useful for existing users
- vLLM/SGLang: useful for later GPU/server mode

## Model Direction

Primary candidate:

- `Qwen3.5-4B` quantized GGUF

Reasons:

- better current fit than Gemma 4 E4B for Korean/code/agent workflows
- 4B leaves more room than 9B for runtime, context, verification, and local UI/process overhead on 16 GB machines
- vision-capable workflows may be possible later

Comparison candidate:

- `Gemma 4 E4B`

Keep it for benchmark comparison, especially multimodal/on-device positioning.

Not default:

- `Qwen3.5-9B`, because it may run on 16 GB when quantized but constrains context and runtime options too much for the target product.

## Korean-Only Requirement

The user strongly requires that user-facing output must not randomly mix other languages.

Runtime should enforce:

- Korean-only final responses
- detect English, Chinese, Japanese leakage
- reject and regenerate mixed-language output
- keep code blocks separate from natural-language language checks
- fail closed with a Korean-only error message if regeneration fails

Model choice alone is not enough for this requirement.

## CLI Shape

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

Expected first-run model flow:

1. User installs `rpotato`.
2. User runs `rpotato init` or `rpotato model install`.
3. CLI checks OS, architecture, RAM, and disk.
4. CLI recommends a model.
5. User explicitly approves download.
6. CLI downloads model with resume support.
7. CLI verifies hash.
8. CLI registers model in local config.
9. CLI starts or reuses the local inference backend.

Model weights should not be bundled into the CLI installer.

## Publishing Direction

Likely initial publishing:

- GitHub repo
- GitHub Releases for binaries
- model manifest in repo or release asset

Likely package channels later:

- Homebrew for macOS/Linux
- Scoop or winget for Windows
- npm wrapper only if useful for adoption

Current implementation lean:

- Rust CLI
- `llama.cpp` sidecar
- backend adapter boundary from day one

TypeScript/Node remains possible for prototyping, but the current reasoning favors Rust for single-binary distribution, process control, and cross-platform packaging.

## Next Session Suggested Start

Start from:

```sh
cd /Users/sys/Desktop/codes/rolling-potato
```

Then inspect:

```sh
ls
sed -n '1,220p' PLAN.md
sed -n '1,260p' HANDOFF.md
```

Suggested next work:

1. Initialize and push the independent Git repository to `https://github.com/MCprotein/rolling-potato.git` if not already done.
2. Choose the exact trusted `Qwen3.5-4B` GGUF artifact and quantization level.
3. Scaffold the Rust CLI around `rpotato init`, `rpotato doctor`, and `rpotato model list`.
4. Implement `llama.cpp` sidecar discovery/health-check before chat behavior.
5. Build the first fixture benchmark for Korean/code/tool reliability.

## User Preference Notes

- User wants direct, practical Korean discussion.
- User is comfortable with technical tradeoffs.
- User wants the product idea shaped before implementation.
- User prefers CLI, not GUI.
- User wants Windows compatibility, so avoid Mac-only defaults.
- User is skeptical of heavy runtimes and wants the runtime to be appropriate for small models.
