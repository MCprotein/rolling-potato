# rolling-potato

<p align="center">
  <strong>ENGLISH</strong> |
  <a href="README.ko.md">한국어</a>
</p>

> Local coding agents for potato PCs.

`rolling-potato` is a local-first coding-agent runtime for low-end and
mid-range machines. It is designed around a simple premise:

> Small models need a small-model runtime, not just a smaller prompt.

| Project snapshot | |
| --- | --- |
| Current release | `v0.41.0` |
| CLI | `rpotato` |
| Runtime | Rust, managed `llama.cpp`, GGUF |
| Primary surfaces | CLI and TUI |
| Release platforms | macOS arm64/x64, Linux arm64/x64, Windows x64 |
| User-facing language | Korean |

[Install](#installation) · [How it works](#how-it-works) ·
[Current capabilities](#current-capabilities) ·
[Safety model](#architecture-and-safety) · [Documentation](#documentation)

---

## Overview

### Why a runtime?

Small local models can be useful, but they are fragile. They need a runtime
that narrows choices, enforces policy, preserves evidence, and manages failure
modes. `rolling-potato` therefore owns more than model inference:

- model and backend lifecycle
- bounded repository context
- permissions, approvals, hooks, and stop gates
- durable sessions, transcripts, workflows, and evidence
- patch preview, application, and verification
- skills, plugin adapters, subagents, and teams
- token, latency, CPU, memory, disk, and benchmark monitoring
- CLI, TUI, and optional local static HTML reporting
- Korean final-response validation

The CLI and TUI are user surfaces. Session state, tool permissions, context,
the agent loop, and verification remain runtime-core responsibilities.

### Who is it for?

The first audience is:

- Korean-speaking developers
- users who find subscription coding agents expensive
- users with 16 GB RAM class low-end or mid-range laptops
- users who want code and model execution to remain local
- users who want local coding help without first mastering local LLM tooling

The long-term target is a practical local alternative to Claude Code/Codex,
not a thin wrapper that imitates them with a weaker model.

---

## Installation

Official binaries are published through
[GitHub Releases](https://github.com/MCprotein/rolling-potato/releases).
Homebrew and Scoop are published package channels. The generated winget
manifest remains an unpublished, validated release artifact.

### Homebrew

Supports macOS arm64/x64 and Linux arm64/x64.

```sh
brew tap MCprotein/rpotato
brew install rpotato
brew update
brew upgrade rpotato
brew uninstall rpotato
```

### Scoop

Supports Windows x64.

```powershell
scoop bucket add rpotato https://github.com/MCprotein/scoop-rpotato
scoop install rpotato/rpotato
scoop update rpotato
scoop uninstall rpotato
```

### winget

No winget community package is published. Install, upgrade, and uninstall
commands are intentionally not advertised.

Package-manager removal deletes the managed executable only. Before removing
it, use `rpotato uninstall --dry-run --purge-cache` to inspect the separate
application-data cleanup plan. The checksum, qualification, recovery, and
publication contracts are documented in [docs/release.md](docs/release.md).

---

## How It Works

The intended local setup flow is:

1. Run `rpotato init` or `rpotato model install`.
2. Inspect the detected OS, architecture, memory, and disk capacity.
3. Install or verify the runtime-managed `llama.cpp` sidecar.
4. Review a source-backed GGUF model recommendation.
5. Approve the download explicitly.
6. Verify the artifact hash and local evidence.
7. Promote and register only a candidate that passes the local gate.
8. Start or reuse the inference backend.

Model weights are not bundled with `rpotato`. A global `llama.cpp`
installation is not required on the managed path.

Start by inspecting the environment and available surfaces:

```sh
rpotato doctor
rpotato init
rpotato model list
rpotato backend doctor
rpotato run "이 저장소의 테스트 실패 원인을 찾아줘"
rpotato tui
```

The detailed MVP acceptance criteria are in [docs/mvp.md](docs/mvp.md).

---

## Current Capabilities

`v0.41.0` is an active pre-1.0 runtime, not only a product-definition
scaffold. Its implemented areas include:

| Area | Current surface |
| --- | --- |
| Agent loop | Intent routing, bounded context, typed model actions, guarded Korean reporting |
| Durable state | Sessions, transcripts, workflows, ledgers, evidence, resume and continue |
| Patch workflow | Preview, explicit apply approval, separate verification approval, rollback records |
| Backend and models | Managed sidecar lifecycle, source-backed candidates, local promotion/install gate |
| Extensions | Native hooks and skills; local Codex/Claude Code plugin adapters |
| Collaboration | One bounded subagent and runtime-owned team execution |
| Monitoring | CLI/TUI metrics, SQLite projection, benchmark records, static HTML export |
| Interfaces | CLI, interactive/read-only TUI, self-contained local HTML report |

See [docs/current-capabilities.md](docs/current-capabilities.md) for the
chaptered capability map, representative commands, and known incomplete
boundaries. Use `rpotato --help` and subcommand help for the exact command
syntax of the installed version.

---

## Architecture and Safety

The runtime treats model output as untrusted input. Model text never executes
tools directly. Every supported side effect must pass runtime policy,
approval where required, evidence recording, and verification.

Key constraints:

- user-facing natural-language output is Korean
- code blocks, paths, commands, and quoted logs remain unchanged when needed
- mixed-language final output gets one regeneration attempt, then fails closed
- imported plugin instructions cannot widen runtime permissions
- shell, background, remote-connector, sensitive-setting, and write
  capabilities remain blocked unless a supported policy path allows them
- uncertain backend requests or verification commands are not replayed
  automatically after recovery
- model, license, performance, memory, and compatibility claims require
  recorded sources or local evidence

Architecture details:

- [Code architecture](docs/code-architecture.md)
- [Runtime architecture](docs/runtime-architecture.md)
- [State lifecycle](docs/state-lifecycle.md)
- [Command policy](docs/command-policy.md)
- [Threat model](docs/threat-model.md)

Qwen and Gemma entries are evaluation candidates, not assumed defaults.
`llama.cpp` is a backend, not a model candidate. See
[model sources](docs/model-source-policy.md),
[model licenses](docs/model-licenses.md), and
[local evaluation](docs/model-eval.md).

---

## Project Status

The release history through `v0.41.0` is complete. The latest release added an
optional self-contained HTML monitoring report backed by the existing local
SQLite/ledger data. It does not add a server, external telemetry, JavaScript,
or network requests.

No version after `v0.41.0` is currently defined. New roadmap work must first
be assigned to a concrete version in [ROADMAP.md](ROADMAP.md).

---

## Documentation

Use the [documentation index](docs/README.md) to browse by topic.

| Start with | Purpose |
| --- | --- |
| [PLAN.md](PLAN.md) | Product intent, target users, and product shape |
| [ROADMAP.md](ROADMAP.md) | Version-only release history and next-version rule |
| [DESIGN.md](DESIGN.md) | CLI, TUI, and monitoring UX source of truth |
| [Current capabilities](docs/current-capabilities.md) | Implemented areas, entry points, and known boundaries |
| [Development](docs/development.md) | Local development and verification workflow |
| [Release](docs/release.md) | Version, branch, artifact, and publication policy |

Korean documentation is available through [README.ko.md](README.ko.md) and
the [Korean documentation index](docs/ko/README.md).

---

## Governance

This is a public Apache-2.0 open-source repository, but external code
contributions and external PRs are not currently accepted. Bug reports,
usability feedback, security reports, and model/license evidence may be
accepted.

See [GOVERNANCE.md](GOVERNANCE.md), [SECURITY.md](SECURITY.md), and
[PRIVACY.md](PRIVACY.md).
