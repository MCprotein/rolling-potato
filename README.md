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
| Current release | `v0.42.0` |
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

Official binaries are distributed only through
[GitHub Releases](https://github.com/MCprotein/rolling-potato/releases).
Download the archive for your platform and verify it with the matching
`.sha256` file or the aggregate checksum file. Homebrew, Scoop, winget, and
other package-manager channels are not operated by this project.

Starting with `v0.42.0`, run the extracted binary once to place it
in the user-local CLI directory and register that directory in the detected
shell profile or Windows user PATH:

```sh
./rpotato install
$HOME/.local/bin/rpotato init
```

On Windows, use `.\rpotato.exe install`, then run
`& "$env:LOCALAPPDATA\Programs\rpotato\bin\rpotato.exe" init`. A new terminal
picks up the persistent PATH automatically; both commands print the one-line
activation command for the current terminal. `RPOTATO_*` variables remain
optional overrides and are not forced globally.

For a reset that removes the global application data and only the current
project's `.rpotato` state, inspect and then explicitly confirm the clean
install:

```sh
./rpotato install --clean --dry-run
./rpotato install --clean --yes
```

Clean install is blocked while a managed backend or generation is active.
Its dry-run also reports whether the binary and PATH registration would be
created, updated, or left unchanged. Runtime publication and deletion share a
cross-process guard; an unavailable process-liveness check blocks deletion.

To remove the program and all runtime-managed state, inspect the exact scope
and then confirm it explicitly:

```sh
rpotato uninstall --clean --dry-run
rpotato uninstall --clean --yes
```

Clean uninstall removes the installed binary, the PATH block owned by
`rpotato`, global application data, and the current project's `.rpotato`.
User-owned files such as the extracted source binary and source repositories
are preserved. On Windows, self-removal of the installed binary completes
immediately after the current process exits.

Supported release targets and checksum verification are documented in
[docs/release.md](docs/release.md).

---

## How It Works

The intended local setup flow is:

1. Run `rpotato`.
2. Review the model/version, quantization, download size, context, RAM status,
   license, and source evidence shown by first-run setup.
3. Select a model and approve its download.
4. Let `rpotato` install or reuse its managed `llama.cpp` backend, verify the
   artifact size and SHA-256, and start the model.
5. Enter a coding request in the same TUI.

Model weights are not bundled with `rpotato`. A global `llama.cpp`
installation is not required on the managed path.

Start the product with:

```sh
rpotato
```

Running `rpotato` without arguments starts the primary TUI. Plain text entered
there is treated as a coding request. The line below the composer shows the
current model, context usage, compaction checkpoint, backend state, and session.
Normal TUI operations use `/model`, `/compact`, `/status`, `/sessions`, `/doctor`,
`/more`, `/back`, `/clear`, `/help`, and `/quit`. Long responses remain available
through `/more` and `/back` instead of being discarded at the viewport boundary.
Context compaction starts automatically at 75% usage; `/compact` creates a manual
checkpoint while preserving the immutable transcript as the authority.

The smaller public CLI surface is:

```sh
rpotato doctor
rpotato init
rpotato run "이 저장소의 테스트 실패 원인을 찾아줘"
rpotato debug --help
```

`rpotato debug --help` lists granular compatibility and diagnostic commands;
normal setup does not require a backend executable or GGUF path.

The detailed MVP acceptance criteria are in [docs/mvp.md](docs/mvp.md).

---

## Current Capabilities

The `v0.42.0` release plus the in-development `v0.43.1` recovery source form an active
pre-1.0 runtime, not only a product-definition scaffold. Implemented areas include:

| Area | Current surface |
| --- | --- |
| Agent loop | Intent routing, bounded context, typed model actions, guarded Korean reporting |
| Durable state | Sessions, transcripts, workflows, ledgers, evidence, resume and continue |
| Patch workflow | Preview, explicit apply approval, separate verification approval, rollback records |
| Backend and models | Managed sidecar lifecycle, source-backed candidates, local promotion/install gate |
| Extensions | Native hooks and skills; local Codex/Claude Code plugin adapters |
| Collaboration | One bounded subagent and runtime-owned team execution |
| Monitoring | CLI/TUI metrics, SQLite projection, benchmark records, static HTML export |
| Interfaces | Primary conversation TUI, automation/diagnostic CLI, self-contained local HTML report |

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

The release history through `v0.42.0` is complete. The latest release adds
user-local self-install, automatic PATH registration, `init` environment
repair, guarded clean reinstall, and a symmetric clean uninstall that
preserves user-owned files. `v0.43.1` is recovering the guided default TUI and
bounded small-model context compaction from the incomplete v0.43.0 binary
publication. See
[ROADMAP.md](ROADMAP.md).

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
