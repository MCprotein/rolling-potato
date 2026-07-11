# Architecture

## MVP Decision

The MVP starts with a Rust runtime core, a CLI surface, and a managed `llama.cpp` sidecar.

This prioritizes the following constraints:

- It must run on a 16 GB RAM laptop.
- It must support macOS and Windows first.
- Installation should stay as simple as possible.
- Model weights must not be bundled into the `rpotato` release binary.
- Small-model fragility should be reduced by runtime policy, not by prompt wording alone.

## Why Rust

Rust is the default implementation language.

Reasons:

- It is suitable for single-binary distribution.
- Cross-platform process control is reliable.
- It fits local file, diff, command execution, and config management work.
- Users do not need a Node runtime installed.
- Future native backend integration remains possible.

TypeScript/Node may be faster for prototyping, but it is less aligned with the MVP goal of lightweight distribution and local process control.

## Why A `llama.cpp` Sidecar

The MVP manages a `llama-server` or equivalent `llama.cpp` executable as a sidecar instead of using a native binding.

Reasons:

- It directly matches the GGUF ecosystem.
- It can target macOS, Windows, and Linux.
- Packaging risk is lower than native bindings.
- Backend failure is isolated from the CLI process.
- It creates a clear boundary for later LM Studio, Ollama, vLLM, or SGLang adapters.

Native bindings should be reconsidered only if:

- sidecar startup latency significantly hurts product experience
- per-platform binary management becomes too complex
- HTTP boundaries become a bottleneck for streaming, cancellation, or token accounting

Users should not need to install `llama.cpp` globally. The MVP path is for the runtime core to download a platform-specific backend binary, verify its checksum, and run it from the app data directory as a child process. If the user configures a custom backend path, that binary is user-owned and `rpotato uninstall` must not delete it.

## Components

```text
user
  ├─ CLI surface: rpotato
  └─ TUI surface: rpotato tui
       ├─ command parser
       ├─ prompt/approval renderer
       ├─ diff/result display
       ├─ subagent/team status display
       └─ Korean final report display

runtime core
  ├─ config manager
  ├─ model manager
  ├─ backend manager
  ├─ state and ledger
  ├─ observability store
  ├─ hook engine
  ├─ skill registry
  ├─ plugin adapter registry
  ├─ ontology and context plane  # typed graph store plus retrieval indexes
  ├─ repo indexer
  ├─ agent loop
  ├─ subagent runtime
  ├─ team runtime
  ├─ tool policy
  ├─ patch manager
  ├─ verifier
  ├─ evidence and stop gate
  ├─ token and resource monitor
  └─ Korean response guard

managed backend
  └─ llama.cpp sidecar
       └─ GGUF model
```

## Repository And Cache Boundaries

Program/runtime assets, cache/data, and project-local state are separated so deletion and reinstall behavior stay predictable.

```text
rpotato app data root/
  config/
  backends/      # managed llama.cpp binaries
  models/        # GGUF model artifacts
  downloads/     # resumable temporary downloads
  manifests/     # model/backend manifests
  logs/
  state/
    observability.sqlite
    runtime-ledger.jsonl
  plugins/
    imported/    # local plugin source snapshots and normalized manifests
    data/        # plugin-owned app data
  cache/

project root/
  .rpotato/      # project-local index/state/evidence, preserved unless explicitly cleaned
```

`rpotato uninstall --keep-cache` removes only program/runtime assets managed by `rpotato`; models, downloads, manifests, logs, and project-local state remain.

`rpotato uninstall --purge-cache` removes app-level cache and models as well, but never deletes source repositories or project-local `.rpotato/` automatically. Every delete path must support `--dry-run` and show the paths before deletion.

## Responsibility Boundaries

### CLI/TUI Surfaces

Surfaces own user experience, but they do not enforce local work policy directly. They connect user requests, approvals, diffs, progress, and final reports to the runtime core.

- command parsing
- user input forwarding
- approval prompt display
- model download progress display
- diff display and patch-approval forwarding
- verification command approval forwarding
- subagent/team status display
- evidence/stop-gate status display
- final Korean response display

### Runtime Core

The runtime core is the product body behind a Claude Code/Codex-style agent experience. CLI, future TUI/IDE surfaces, and test harnesses should all reuse the same policy and state.

- read and write config
- manage session state and append-only ledgers
- manage SQLite monitoring projection
- manage hook lifecycle
- manage skill registry and invocation
- import, validate, enable, and disable Claude Code/Codex-style plugins
- interpret model manifests
- download, hash-verify, and register models
- start, restart, and stop sidecars
- read project files and pack context
- manage ontology lifecycle as typed graph records with source refs, ledger events, and query indexes
- manage subagent lifecycle
- coordinate team runtime
- enforce tool permission policy
- generate and apply diffs
- classify and run verification commands
- collect evidence and evaluate the stop gate
- collect token, latency, memory, and backend health metrics
- validate final Korean responses

### Backend Adapter

Backend adapters hide inference-backend differences.

The common interface should cover:

- health check
- model metadata
- chat completion
- streaming tokens
- cancellation
- context length reporting
- backend diagnostics

The MVP implements only the `llama.cpp` adapter.

### Plugin Adapter

Plugin adapters convert foreign agent-runtime plugin packages into `rpotato` capabilities.

Goals:

- convert Codex plugin skill/MCP data into `rpotato` skill/MCP capabilities
- convert Claude Code plugin skills, commands, agents, hooks, and MCP data into `rpotato` capabilities
- later review LSP, monitors, `bin/`, settings, and theme/output-style data while keeping risky capabilities blocked by default
- import only local plugin directories
- reject remote URLs, marketplaces, registries, catalogs, and mirrors
- record unsupported capabilities instead of executing them silently

Plugin adapters do not execute foreign plugins directly. Codex adapter comes first; Claude Code adapter follows. Only capabilities that pass import, inspect, validate, enable, tool policy, hook policy, ledger, and evidence gates can run.

See [plugin-adapters.md](plugin-adapters.md).

### Agent Loop

The first vertical slice uses a sequential agent loop.

Default stages:

1. planner: create a short task plan
2. executor: propose a small patch or command
3. verifier: inspect diffs, command output, and logs
4. reporter: produce the final Korean report

For small models, the following matter more than unconditional parallel agents:

- constrained output format per stage
- constrained context size
- short retry after failure
- one verifiable action at a time

Subagents and team execution are required for a replacement-level runtime, but they must share the parent workflow, hooks, policy, evidence, and stop gate.

## Safety Model

Default policy is conservative.

- Project-internal file reads are allowed.
- File writes require user approval after diff display.
- Commands with side effects require user approval.
- Model downloads run only after approval through the CLI surface.
- Foreign plugin import and enablement require local path checks, capability reports, and permission review.
- Operation logs are recorded.
- `doctor` checks environment, backend, and model state.

Trust modes can be added later, but they are not the MVP default.

## Korean Response Guard

Korean-only output is not handled by prompt instruction alone.

Guard steps:

1. Split response into code blocks and natural-language blocks.
2. Detect English, Chinese, and Japanese leakage in natural-language blocks.
3. Apply allowlists:
   - commands
   - file paths
   - package names
   - code identifiers
   - quoted original logs
4. Regenerate once with stricter instruction if leakage is detected.
5. Fail closed with a Korean error if regeneration still fails.

This guard is mandatory for final reporter output. Intermediate model output may be looser, but it should not be exposed directly to users.

## Model Manifest

Model metadata is managed through manifests.

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

`null` and `TODO` are schema placeholders, not product facts.

Qwen and Gemma have source-recorded static `unverified` artifact candidates in the scaffold manifest. Static status is never rewritten by one machine's result. A host may install and select a candidate only while separate local promotion evidence revalidates the artifact bytes, exact backend chat provenance, RAM/mmproj result, and hash-pinned canonical benchmark. Model claims follow [model-source-policy.md](model-source-policy.md).

## Later Adapters

The following backends are reviewed after MVP:

- LM Studio: useful for users who already have it installed and for demos
- Ollama: popular, but too heavy and opaque as the core runtime
- vLLM/SGLang: useful for GPU or server mode

These adapters are extension paths, not the default experience.
