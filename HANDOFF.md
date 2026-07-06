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
- Rust runtime core, CLI surface, and `llama.cpp` sidecar architecture
- MVP acceptance criteria
- Korean/code/tool model evaluation draft
- runtime architecture and terminology
- ontology runtime design
- observability and monitoring design
- hooks, skills, subagents, team runtime, and TUI design
- TUI monitoring UX direction with optional later HTML report/dashboard
- Claude Code/Codex-style plugin adapter design

Open-source operating docs have also been added:

- `AGENTS.md`
- `LICENSE`
- `GOVERNANCE.md`
- `MAINTAINERS.md`
- `SECURITY.md`
- `PRIVACY.md`
- `ROADMAP.md`
- `DESIGN.md`
- `.github/ISSUE_TEMPLATE/`
- `docs/development.md`
- `docs/release.md`
- `docs/model-manifest.md`
- `docs/model-licenses.md`
- `docs/model-source-policy.md`
- `docs/backend-adapters.md`
- `docs/command-policy.md`
- `docs/korean-output-guard.md`
- `docs/threat-model.md`
- `docs/benchmarks.md`
- `docs/runtime-architecture.md`
- `docs/glossary.md`
- `docs/ontology-runtime.md`
- `docs/observability.md`
- `docs/hooks.md`
- `docs/skills.md`
- `docs/subagents.md`
- `docs/team-runtime.md`
- `docs/tui.md`
- `docs/plugin-adapters.md`

External code contributions and external PRs are not accepted for now. This is now recorded in `AGENTS.md`, `GOVERNANCE.md`, `MAINTAINERS.md`, README, and the GitHub issue templates.

Persistent automation instruction:

- Continue making safe, verified changes without asking for repeated confirmation.
- Commit and push meaningful units automatically to `origin main`.
- Use Conventional Commit messages in the form `type(scope): title`, for example `docs(governance): add open source operating policy`.
- Do not record model-related facts without explicit sources. Model names, licenses, artifact URLs, checksums, RAM requirements, backend compatibility, multimodal support, and quality claims must follow `docs/model-source-policy.md`.

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

Build a local-first coding agent runtime for small local models. The first surface is the `rpotato` CLI, but the product body is the runtime core.

This is not just a prompt harness and not merely a model downloader CLI. The user specifically corrected the framing:

> It should be runtime plus harness integration. Small models probably need their own runtime. Claude Code is designed for large models.

Accepted thesis:

> Small models need a small-model runtime, not just a smaller prompt.

The product should make small models reliable by narrowing choices, managing context, maintaining ontology, validating actions, controlling retries, and enforcing Korean-only output.

The user clarified the target: this runtime should be usable instead of Claude Code/Codex. Therefore hooks, skills, subagents, team runtime, and TUI are required product capabilities, not optional extras.

Standing boundary:

- CLI surface displays, prompts, and forwards user intent.
- Runtime core owns state, policy, ontology, context, agent loop, evidence, and stop gates.
- Runtime core owns model/token/resource monitoring with a local SQLite projection and append-only ledger.
- Runtime core owns hooks, skills, subagent lifecycle, team coordination, and TUI state feeds.
- Runtime core owns foreign plugin import, validation, permission reporting, and adapter execution boundaries.
- Backend adapter owns inference calls only.
- Model output never executes tools directly.
- Claude Code/Codex-style plugins are import targets, not trusted runtime code.
- Plugin adapter priority is Codex first, then Claude Code.
- Foreign plugin shell, `bin/`, MCP server, background process, remote connector, and file write paths are blocked by default until explicitly enabled.
- External marketplace, registry, catalog, and mirror support is out of scope.
- Plugin use should happen through local plugin directory import only.

## Target Users

- Korean-speaking users first
- users who find Claude Code / Codex subscriptions expensive
- users with low-end or mid-range laptops
- users who want local/private execution
- non-expert or semi-technical users

Initial machine target:

- 16 GB RAM laptop
- macOS and Windows first
- Linux later, maintainer-led or after governance policy changes

## Runtime Direction

Default runtime direction:

- `llama.cpp`
- GGUF model format
- managed sidecar process or later native integration
- cross-platform runtime distribution with CLI surface

Rejected as default:

- MLX: too Mac-specific
- vLLM: better as server/GPU backend, not low-end local default
- Tauri/Electron: GUI app stacks; MVP starts with Claude Code-like CLI surface
- Ollama as core runtime: convenient, but too heavy/opaque for a tight integrated runtime

Adapters to consider later:

- LM Studio: useful for demo, marketing, and users who already use it
- Ollama: useful for existing users
- vLLM/SGLang: useful for later GPU/server mode

## Model Direction

Primary candidate:

- `Qwen3.5-4B` quantized GGUF

Status:

- user-directed priority evaluation candidate
- not a confirmed default model
- exact GGUF artifact, artifact provider terms, checksum, Korean/code quality, multimodal support, and 16 GB runtime fit are unverified
- do not make model claims without explicit sources

Comparison candidate:

- `Gemma 4 E4B`

Keep it for benchmark comparison only. Artifact, artifact provider terms, multimodal support, and runtime fit are unverified until source-backed evaluation is complete.

Not default:

- `Qwen3.5-9B`, because larger local models may increase pressure on context and runtime options. Exact viability is unverified and requires measurement.

## Korean-Only Requirement

The user strongly requires that user-facing output must not randomly mix other languages.

Runtime should enforce:

- Korean-only final responses
- detect English, Chinese, Japanese leakage
- reject and regenerate mixed-language output
- keep code blocks separate from natural-language language checks
- fail closed with a Korean-only error message if regeneration fails

Model choice alone is not enough for this requirement.

## Runtime Surface Shape

MVP surface: `rpotato` CLI.

Initial command sketch:

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato tui
rpotato skill list
rpotato skill run fix-test
rpotato plugin import --from codex ./my-plugin
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato plugin validate imported.example-plugin
rpotato team status
rpotato model list
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato backend install-plan
rpotato backend install
rpotato backend verify-archive /path/to/llama.cpp.tar.gz --sha256 <64-hex>
rpotato backend health-check
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

Expected first-run model flow:

1. User installs `rpotato`.
2. User runs `rpotato init` or `rpotato model install`.
3. CLI surface forwards the request to runtime core.
4. Runtime core checks OS, architecture, RAM, and disk.
5. Runtime core recommends a source-verified model only after manifest validation.
6. CLI surface asks the user to explicitly approve download.
7. Runtime core downloads model with resume support.
8. Runtime core verifies hash.
9. Runtime core registers model in local config.
10. Runtime core starts or reuses the local inference backend.

Model weights should not be bundled into the `rpotato` release artifact.

## Publishing Direction

Likely initial publishing:

- GitHub repo
- GitHub Releases for binaries
- model manifest in repo or release asset
- local SQLite observability store for runtime/model metrics

Likely package channels later:

- Homebrew for macOS/Linux
- Scoop or winget for Windows
- npm wrapper only if useful for adoption

Current implementation lean:

- Rust runtime core with CLI surface
- TUI surface after the first CLI vertical slice
- `llama.cpp` sidecar with source-backed `b9878` CPU release artifact manifest
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

1. Keep the CLI surface/runtime core/backend boundaries aligned with `docs/runtime-architecture.md`.
2. Keep hooks, skills, subagents, team runtime, and TUI aligned with their docs before adding complex agent behavior.
3. Keep monitoring TUI UX aligned with `DESIGN.md`; HTML is optional later and must share the same observability source.
4. Keep plugin adapter work aligned with `docs/plugin-adapters.md`; start with inspect/validate before execution.
5. Do not add plugin marketplace integration; reject marketplace, registry, catalog, mirror, and remote URL plugin sources.
6. Split the current scaffold toward explicit runtime core modules.
7. Add runtime state, ledger, and observability boundaries before chat behavior.
8. Add managed `llama.cpp` sidecar process startup, startup timeout handling, and stderr/stdout capture.
9. Choose the exact trusted `Qwen3.5-4B` GGUF artifact and quantization level only with source-backed URL, checksum, provider terms, file size, and backend compatibility evidence.

## User Preference Notes

- User wants direct, practical Korean discussion.
- User is comfortable with technical tradeoffs.
- User wants the product idea shaped before implementation.
- User wants a Claude Code/Codex/Gaja Code-like runtime. CLI is the first surface, not the whole product.
- Hooks, skills, subagents, team runtime, and TUI are required for the target product.
- Claude Code/Codex-style plugin compatibility is desirable through an adapter layer, not direct foreign runtime execution.
- User chose Codex plugin compatibility first and Claude Code compatibility second.
- User wants all plugin capabilities considered eventually, with risky external capabilities blocked by default and unlocked through explicit prompts.
- User wants Windows compatibility, so avoid Mac-only defaults.
- User is skeptical of heavy runtimes and wants the runtime to be appropriate for small models.
