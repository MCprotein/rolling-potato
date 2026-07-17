# Roadmap

This roadmap is version-only. New roadmap work must be added as a concrete
version row.

The execution order, release cycle, and non-skippable gates for the
`v0.29.0`-`v0.41.0` train are defined in
[docs/release-train.md](docs/release-train.md). The v0.29.0 release-blocking
foundation corrections, retained by v0.29.1, are recorded in
[docs/v0.29-correction-plan.md](docs/v0.29-correction-plan.md).

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
| v0.23.0 | Released | Official binary download foundation: GitHub Release binary workflow for macOS Apple Silicon and Windows x86_64, release asset checksums, and `rpotato doctor` binary smoke |
| v0.23.1 | Released | Windows binary link fix: `rusqlite` uses bundled Windows SQLite linkage so release assets do not depend on runner-provided `sqlite3.lib` |
| v0.24.0 | Released | Cross-platform release hardening: macOS Intel artifact, aggregate checksum publication, Windows keep-cache/purge-cache uninstall smoke, release notes template |
| v0.24.1 | Released | Checksum workflow fix: basename-only `.sha256` paths, aggregate release upload repository context, checksum path smoke guard |
| v0.24.2 | Released | Aggregate checksum checkout fix: checksum job checks out repo before running repo-local checksum guard |
| v0.25.0 | Released | Verified model install gate: source-backed candidates require local promotion evidence, artifact checksum/size, backend smoke ledger, RAM/mmproj evidence, measured benchmark row before registry registration |
| v0.26.0 | Released | Ontology runtime store: project-local canonical typed graph JSONL, Layer A seed, compact context views, source-pointer reread rules, import/export inspection views |
| v0.27.0 | Released | Plugin adapter hardening: Codex/Claude Code local-directory import records source manifest and snapshot hashes, maps capabilities, keeps shell/MCP/background/runtime-setting permissions blocked by default, and blocks validate/enable on source drift |
| v0.28.0 | Superseded | Linux release artifact matrix introduced, but the first publication was interrupted on the GitHub ARM runner before Linux assets and aggregate checksums completed |
| v0.28.1 | Superseded | Release test gate added, but Linux test execution still exited with signal 143 before artifact publication |
| v0.28.2 | Superseded | Added sidecar timeout cleanup, but release test gate still received a GitHub runner shutdown signal before artifact publication |
| v0.28.3 | Superseded | Serialized release test gate still hit GitHub runner shutdown after the sidecar timeout fixture, before artifact publication |
| v0.28.4 | Superseded | Release-runner-safe skip gate still failed because the stale-record test exposed a Unix PID wrap hazard |
| v0.28.5 | Released | Complete Linux/macOS/Windows release artifacts: Unix PID guard for stale sidecar records, restored full serialized release gate, per-target build/smoke/package jobs, Linux x86_64 and Linux ARM64 tarballs, and aggregate checksum publication |
| v0.29.0 | Superseded | Durable single-agent runtime correction shipped, but the Windows `.sha256` CRLF ending made the aggregate checksum fail Unix `shasum -c` validation |
| v0.29.1 | Released | Cross-platform aggregate checksum fix: explicit ASCII/LF Windows checksum output plus LF/CRLF regression guards, retaining the v0.29.0 runtime correction |
| v0.30.0 | Released | Verified model adoption: Qwen/Gemma pinned-artifact local evaluation, canonical chat/benchmark/RAM/mmproj provenance gate, managed registry install, fail-closed persistent default selection, and derived project-ledger recovery shipped without bundling weights |
| v0.31.0 | Superseded | Backend streaming and cancellation shipped, but the first release was incomplete because the Windows artifact failed during sidecar stop fallback |
| v0.31.1 | Released | Windows sidecar stop fallback restored the complete five-platform artifact set while retaining v0.31.0 streaming and cancellation behavior |
| v0.32.0 | Released | Durable conversation resume: canonical user/visible-model/tool/evidence transcripts, ledger-ordered rebuildable SQLite projection, one shared bounded source-context budget, preflight-before-mutation session selection, and idempotent `resume`/`continue` without uncertain side-effect replay |
| v0.32.1 | Released | Stable toolchain refresh: Rust 1.97.0, current stable Cargo resolution, Node.js 24 GitHub Actions, current GA hosted runners, and source-pinned llama.cpp b9982 artifacts with verified install provenance |
| v0.33.0 | Released | Executable hooks and skills: lifecycle hooks and built-in skill state machines run inside the durable agent loop with deterministic ordering, fail-closed results, policy enforcement, evidence, and stop criteria |
| v0.34.0 | Superseded | Implemented the runtime-owned interactive TUI, pending-action approve/deny, diff and tool-output inspection, workflow resume/cancel, and recoverable exact prepared transactions, but only partially published binaries |
| v0.34.1 | Superseded | Recovered portable Windows file identity and Linux ARM64 source handling, but only partially published binaries |
| v0.34.2 | Superseded | Recovered Windows ConPTY lifecycle, long-path atomic replacement, and bounded native fixtures, but only partially published binaries |
| v0.34.3 | Released | Recovered the native release gate with bounded platform sampling, status-line health probes, the Rust fake sidecar, and graceful TCP half-close, then published the verified exact 11-asset set |
| v0.35.0 | Superseded | Bounded subagent source shipped, but the release test gate inherited the real release tag into an ordinary-PR policy fixture and stopped before binary builds |
| v0.35.1 | Released | Hermetic release-contract fixture that clears ambient tag context while retaining the complete v0.35 bounded subagent implementation |
| v0.36.0 | Released | Team execution: dispatch admitted lanes, advance team stages, enforce ownership at action time, reconcile results, handle failed lanes, and apply verification/stop gates before completion |
| v0.37.0 | Released | Codex plugin execution adapter: execute supported locally imported capabilities through native policy/hook/skill boundaries while shell, MCP, background, remote, and write capabilities remain blocked until explicit approval |
| v0.37.1 | Consolidated | Unpublished architecture-foundation implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.2 | Consolidated | Unpublished foundation-and-platform implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.3 | Consolidated | Unpublished inference-boundary implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.4 | Consolidated | Unpublished workflow-storage-compatibility implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.5 | Consolidated | Unpublished validated-domain-view implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.6 | Consolidated | Unpublished workflow-transaction-and-recovery implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.7 | Consolidated | Unpublished observability-boundary implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.8 | Consolidated | Unpublished knowledge-and-policy implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.9 | Consolidated | Unpublished patch-boundary implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.10 | Consolidated | Unpublished runtime-and-reporting implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.11 | Consolidated | Unpublished extension-boundary implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.12 | Consolidated | Unpublished collaboration-boundary implementation milestone included in the exact-tree v0.37.13 release |
| v0.37.13 | Released | Complete behavior-preserving architecture ownership migration: private `app`/`composition`/`surfaces`/`runtime_core`/`adapters`/`foundation` roots, thin binary entrypoint, zero root compatibility facades, and complete migration ledger |
| v0.38.0 | Planned | Claude Code plugin execution adapter: map supported local capabilities onto the established native adapter contract, report unsupported semantics, and preserve the same default-deny permission boundary |
| v0.39.0 | Planned | Integrated performance hardening: benchmark completed agent/subagent/team workflows, optimize CPU/RSS/context/token usage from measured evidence, and promote regressions into reproducible fixtures without unsupported model claims |
| v0.40.0 | Planned | Package-manager distribution: decide and implement maintainable Homebrew/Scoop/winget channels against signed or checksummed GitHub Release assets, with install/upgrade/uninstall validation |
| v0.41.0 | Planned | Optional local HTML monitoring report: export or serve a local-only SQLite/ledger-backed dashboard with redaction and no second telemetry source of truth |
