# Roadmap

This roadmap is version-only. New roadmap work must be added as a concrete
version row.

`rolling-potato` is a local coding-agent runtime for small local models. The
runtime owns instruction, permission, state, ledger, transcript, evidence,
monitoring, ontology-backed context, plugins, hooks, subagents, teams, and TUI
surfaces. Public model claims and benchmark claims must stay within recorded
evidence.

| Version | Status | Scope |
| --- | --- | --- |
| v0.1.0 | Released | Developer preview: CLI scaffold, source-only release, initial runtime contract notes |
| v0.2.0 | Released | `run` skeleton preview: context-aware model-response skeleton, model-run metrics, non-executing model action parsing |
| v0.3.0 | Released | Patch diff approval preview: proposal records, diff display, approval gate foundation |
| v0.4.0 | Released | Approved patch apply: recorded approval token, allowed verification command, guarded patch apply |
| v0.5.0 | Released | Read-only TUI beta: terminal surface foundation |
| v0.6.0 | Released | TUI approval and diff views |
| v0.7.0 | Released | TUI session transcript view |
| v0.8.0 | Released | TUI evidence and stop-gate view |
| v0.9.0 | Released | Backend resource sampling: sidecar CPU/RSS/memory/disk sampling, local ledger/SQLite recording, CLI status fields |
| v0.10.0 | Released | TUI resource monitor: CPU, memory, latency, token throughput, resource-pressure status |
| v0.11.0 | Released | Backend chat resource governor: backend health/resource thresholds, critical-pressure block, degraded-pressure max-token clamp, CLI/ledger reporting |
| v0.12.0 | Released | Read-only team admission preview: latest resource sample, parallel admission, sequential fallback, blocked dispatch |
| v0.13.0 | Released | Team admission gate: requested lane enforcement, sequential fallback, critical-pressure block, ledger recording |
| v0.14.0 | Released | Team policy preflight: requested write paths and commands checked before dispatch; ask/deny blocks worker launch |
| v0.15.0 | Released | Team file ownership preflight: lane-owned write paths normalized before dispatch; cross-lane conflicts block worker launch |
| v0.16.0 | Released | Team approval queue integration: blocked policy/ownership decisions write approval request records and appear in `tui approvals` |
| v0.17.0 | Released | Team context and model governor: requested context clamp, resource-sensitive model route hints, ledger recording |
| v0.18.0 | Released | Performance baseline report: p50/p95 latency, tokens/sec, context clamp count, peak RSS, pressure state, backend/model/session grouping |
| v0.19.0 | Released | Benchmark harness foundation: fixture schema validation, benchmark run ledger/projection, reproducibility metadata, redacted local report export |
| v0.20.0 | Released | Executable benchmark runner: prompt artifact execution through active backend sidecar, local 0-3 score, `measured-locally` benchmark rows, model/token/resource metric linkage |
| v0.20.1 | Released | Benchmark evidence status: real Qwen executable smoke measurement documented; `model eval-plan` surfaces latest local measured benchmark row |
| v0.21.0 | Released | Benchmark-driven optimization policy: `monitor optimize` recommends context budget, lane count, fallback, and model route from measured local metrics and benchmark evidence |
| v0.22.0 | Released | Dispatcher hardening: `team dispatch` enforces dispatch-time file ownership, records failed-worker continuation, and surfaces latest team runtime status |
| v0.23.0 | Planned | Official binary download foundation: release build pipeline, GitHub Release assets, macOS Apple Silicon and Windows x86_64 `rpotato` artifacts, binary checksums, `rpotato doctor` release smoke |
| v0.24.0 | Planned | Cross-platform release hardening: macOS Intel artifact, checksum publication, Windows keep-cache/purge-cache uninstall smoke, release notes template |
| v0.25.0 | Planned | Verified model install path: promote source-backed candidates only after local evidence, registry registration, install download flow, RAM/mmproj evidence gate |
| v0.26.0 | Planned | Ontology runtime store: canonical internal typed graph/store, compact context views, source-pointer reread rules, import/export inspection views |
| v0.27.0 | Planned | Plugin adapter hardening: Codex local-directory import completion, Claude Code adapter follow-up, default-deny external commands/MCP/background permissions |
| v0.28.0+ | Planned | Post-MVP packaging decisions: Homebrew/Scoop/winget decision, Linux x86_64 and Linux ARM64 artifacts, optional local HTML report/dashboard |
