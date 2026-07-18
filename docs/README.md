# Documentation

English is the default repository documentation language.

[Project README](../README.md) · [한국어 문서](ko/README.md)

## Choose a Reading Path

| Goal | Reading order |
| --- | --- |
| Understand the product | [Plan](../PLAN.md) → [Roadmap](../ROADMAP.md) → [Current capabilities](current-capabilities.md) |
| Understand the code | [Code architecture](code-architecture.md) → [Runtime architecture](runtime-architecture.md) → [State lifecycle](state-lifecycle.md) |
| Build or release | [Development](development.md) → [Release](release.md) → [Release train](release-train.md) |
| Evaluate a model | [Source policy](model-source-policy.md) → [Manifest](model-manifest.md) → [Evaluation](model-eval.md) → [Benchmarks](benchmarks.md) |

## Product and Experience

- [Product plan](../PLAN.md) — intent, users, product shape, and MVP direction
- [Version roadmap](../ROADMAP.md) — release history and the next-version rule
- [Design source of truth](../DESIGN.md) — CLI, TUI, and monitoring experience
- [Current capabilities](current-capabilities.md) — implemented areas, entry
  points, and known boundaries
- [MVP acceptance criteria](mvp.md) — first useful product contract
- [CLI output style](cli-output-style.md) — concise, evidence-oriented terminal
  output
- [Glossary](glossary.md) — canonical project terminology

## Architecture and State

- [Architecture](architecture.md) — overall product and runtime boundaries
- [Code architecture](code-architecture.md) — module ownership and dependency
  direction
- [Runtime architecture](runtime-architecture.md) — surface, core, adapter, and
  artifact layers
- [State lifecycle](state-lifecycle.md) — canonical state, projection,
  recovery, and resume
- [Ontology runtime](ontology-runtime.md) — typed project knowledge and
  source-pointer rereads
- [Observability](observability.md) — ledger, SQLite projection, metrics, and
  retention

## Runtime Capabilities

- [Backend adapters](backend-adapters.md)
- [Command policy](command-policy.md)
- [Hooks](hooks.md)
- [Skills](skills.md)
- [Subagents](subagents.md)
- [Team runtime](team-runtime.md)
- [TUI](tui.md)
- [Plugin adapters](plugin-adapters.md)
- [Korean output guard](korean-output-guard.md)

## Models and Evaluation

- [Model source policy](model-source-policy.md)
- [Model manifest](model-manifest.md)
- [Model knowledge base](model-knowledge-base.md)
- [Model licenses](model-licenses.md)
- [Model evaluation](model-eval.md)
- [Benchmarks](benchmarks.md)

Model names, licenses, performance, memory fit, backend compatibility, and
multimodal claims require cited or locally measured evidence. Unverified
claims remain explicitly unverified.

## Development and Release

- [Development](development.md)
- [Release policy and workflow](release.md)
- [Release train](release-train.md)
- [Release notes](../RELEASE_NOTES.md)
- [Release-notes template](release-notes-template.md)
- [v0.29 correction plan](v0.29-correction-plan.md) — retained historical
  release-blocking corrections

## Security, Privacy, and Governance

- [Threat model](threat-model.md)
- [Security policy](../SECURITY.md)
- [Privacy policy](../PRIVACY.md)
- [Governance](../GOVERNANCE.md)
- [Maintainers](../MAINTAINERS.md)

## Maintainer References

- [Agent handoff](../HANDOFF.md)
- [Agent execution retrospectives](agent-retrospectives.md)
- [Repository agent instructions](../AGENTS.md)
