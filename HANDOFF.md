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

- `Qwen3.5-4B` Q4_K_M GGUF from `unsloth/Qwen3.5-4B-GGUF`

Status:

- user-directed priority evaluation candidate
- not a confirmed default model
- exact GGUF artifact URL, provider page, LFS SHA-256, and file size are source-recorded as `unverified`
- Qwen artifact download and local `llama.cpp b9878` lifecycle smoke with `--ctx-size 4096` are completed
- Korean/code quality, text-only mmproj need, multimodal support, Gemma comparison, and 16 GB runtime fit are unverified
- do not make model claims without explicit sources

Source-recorded artifact facts checked 2026-07-06:

- provider: `unsloth/Qwen3.5-4B-GGUF`
- artifact: `Qwen3.5-4B-Q4_K_M.gguf`
- pinned revision: `e87f176479d0855a907a41277aca2f8ee7a09523`
- size bytes: `2740937888`
- expected SHA-256 from Hugging Face LFS oid: `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`
- sources: https://huggingface.co/api/models/Qwen/Qwen3.5-4B, https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF, https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF/tree/main?recursive=1

Local execution evidence checked 2026-07-06:

- `rpotato model fetch-candidate qwen3.5-4b --for-evaluation` downloaded and verified the Qwen artifact by size and SHA-256, but did not register it as installed.
- `rpotato backend install` installed managed `llama.cpp b9878`.
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096` started the managed sidecar, `backend status` reported `ctx size: 4096` and healthy, `/health` returned HTTP 200, and `backend stop` stopped the sidecar.
- Raw `/completion` generated tokens through the managed sidecar, but exposed reasoning trace text and did not prove clean Korean final-answer quality.
- Qwen official model card says Qwen3.5 thinks by default and direct response requires API parameters, not the Qwen3 `/think` or `/nothink` soft switches. Source: https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode, checked 2026-07-06.
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64` called `/v1/chat/completions` with `chat_template_kwargs.enable_thinking=false` and returned `guard: pass`, `finish reason: stop`, `prompt tokens: 57`, `completion tokens: 16`, `total tokens: 73`, and clean response `감자는 땅속에서 자라는 식물의 뿌리줄기입니다.`
- `v0.2.0` work started on `release/v0.2.0`: `rpotato run` now performs deterministic routing, builds a bounded repository context pack with source pointers, prepares a runtime-owned action candidate/next gate, calls the running backend sidecar for a context-aware model-response agent-loop skeleton, parses the model's structured action line or recognized action text without execution, and records model/token metrics in SQLite. Latest verified model-action smoke read `src/intent.rs:1`, `src/app.rs:1`, `src/backend.rs:1`, and `src/cache.rs:1`; it returned `action candidate: patch-proposal`, `model action parse: heuristic-text`, `model action kind: patch-proposal`, `model action executable now: no`, `next gate: diff-before-write`, `guard: pass`, `finish reason: stop`, `prompt tokens: 1482`, `completion tokens: 72`, `total tokens: 1554`; `monitor models` showed `Qwen3.5-4B-Q4_K_M: runs 6, prompt 6032, completion 311, total 6343, avg latency 1303.0ms`.
- `v0.3.0` work started on `release/v0.3.0`: `rpotato patch preview --path <path> --find <text> --replace <text>` renders a unified diff, writes a proposal record under `.rpotato/patch-proposals/`, prints an approval token, and does not modify the target file. `rpotato patch approve <proposal-id> --token <token> --dry-run` verifies the approval gate and records a ledger event without applying the patch. Latest smoke previewed `RELEASE_NOTES.md` from `Run Skeleton Preview` to `Run Skeleton Preview Smoke`, returned `status: diff-ready`, then approve returned `status: gate-passed`; `git diff -- RELEASE_NOTES.md` was empty after smoke.
- `v0.4.0` work started on `release/v0.4.0`: `rpotato patch approve <proposal-id> --token <token>` now applies approved proposals when the current target SHA-256 still matches the previewed original SHA-256, writes a rollback record, verifies the applied SHA-256, and can run `--verify-command <command>` for policy-allowed simple argv verification commands. Latest scratch smoke used `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke`, previewed `README.md` from `Local coding agents for potato PCs.` to `Local coding agents for potato PCs. Smoke`, then approved with `--verify-command "rg Smoke README.md"`; output returned `status: applied`, rollback record path, `verification status: passed`, and verification exit code `0`.
- `v0.5.0` work started on `release/v0.5.0`: `rpotato tui`, `rpotato tui monitor`, and `rpotato tui sessions` render dependency-free read-only ASCII TUI beta views from existing runtime state and the SQLite observability projection. Latest smoke returned overview, monitor, and sessions dashboards showing project/session state, observability path, recorded Qwen token metrics, full session ids, and the read-only beta boundary.
- `v0.6.0` work started on `release/v0.6.0`: `rpotato tui approvals` and `rpotato tui diff <proposal-id>` render read-only patch proposal approval/diff views from `.rpotato/patch-proposals/` records. Latest scratch smoke used `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v0.6-smoke`, previewed `src/lib.rs` from `1` to `2`, listed `patch-proposal-bae4b383a107e485` as `pending-approval`, and showed the stored unified diff without approving or applying the patch; `COLUMNS=64` kept the diff readable.

Comparison candidate:

- Google `Gemma 4 E4B` IT QAT q4_0 GGUF

Keep it for benchmark comparison only. Its artifact URL, LFS SHA-256, and file size are source-recorded as `unverified`; multimodal support, text-only mmproj need, and runtime fit are unverified until local evaluation is complete.

Source-recorded artifact facts checked 2026-07-06:

- provider: `google/gemma-4-E4B-it-qat-q4_0-gguf`
- artifact: `gemma-4-E4B_q4_0-it.gguf`
- pinned revision: `bb3b92e6f031fa438b409f898dd9f14f499a0cb0`
- size bytes: `5154939136`
- expected SHA-256 from Hugging Face LFS oid: `e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d`
- sources: https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-unquantized, https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf, https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf/tree/main?recursive=1

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
rpotato model eval-plan qwen3.5-4b
rpotato model benchmark-plan qwen3.5-4b
rpotato model fetch-candidate qwen3.5-4b --for-evaluation
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato backend install-plan
rpotato backend install
rpotato backend start --model "/path/to/model.gguf" --ctx-size 4096
rpotato backend verify-archive /path/to/llama.cpp.tar.gz --sha256 <64-hex>
rpotato backend health-check
rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64
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
7. Extend the TUI beyond read-only beta: transcript/session view, tool output viewer, subagent/team status, plugin permission review, and stop-gate evidence view.
8. Connect model action output to the patch preview/apply flow, then add verification output interpretation, final Korean reporting, and stop gate evidence checks.
9. Add backend streaming response handling and generation cancellation on top of the managed sidecar lifecycle.
10. Run `rpotato model eval-plan <id>` before local model work to check source-backed fields, app-data artifact presence, and the next smoke/benchmark step.
11. Run `rpotato model benchmark-plan <id>` before assigning any score so public benchmark parity conditions and local product benchmark gates remain separated.
12. Run `rpotato model fetch-candidate <id> --for-evaluation` only when intentionally downloading multi-GB candidate artifacts for local evaluation.
13. Run Qwen final-answer benchmark fixtures through `rpotato backend chat` before assigning model-quality scores.
14. Run Gemma evaluation artifact fetch and local backend smoke with an explicit context limit, for example `rpotato backend start --model <path> --ctx-size 4096`.
15. Measure RAM-fit/mmproj-need for the source-recorded Qwen/Gemma GGUF artifact candidates.
16. Keep `model install` blocked until verified install download, benchmark evidence, and registry registration gates are complete.

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
- When reporting Codex goal completion, include `tokensUsed`, elapsed time, and Codex Pro $100 usage percentage only if an official/public session-token denominator or explicit goal token budget is available. Current official OpenAI docs state Pro $100 has 5x higher usage than Plus, but do not publish a session-token denominator; do not invent one. Source checked 2026-07-07: https://help.openai.com/en/articles/9793128-about-chatgpt-pro-plans
