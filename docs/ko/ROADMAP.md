# 로드맵

이 로드맵은 버전 전용으로 관리합니다. 새 로드맵 항목은 반드시 구체적인
버전 행으로 추가합니다.

`rolling-potato`는 작은 로컬 모델용 coding-agent runtime입니다. Runtime은
instruction, permission, state, ledger, transcript, evidence, monitoring,
ontology 기반 context, plugin, hook, subagent, team, TUI surface를 소유합니다.
공개 모델 claim과 benchmark claim은 기록된 evidence를 넘어가면 안 됩니다.

| Version | 상태 | 범위 |
| --- | --- | --- |
| v0.1.0 | Released | Developer preview: CLI scaffold, source-only release, 초기 runtime contract note |
| v0.2.0 | Released | `run` skeleton preview: context-aware model-response skeleton, model-run metric, model action 비실행 parsing |
| v0.3.0 | Released | Patch diff approval preview: proposal record, diff display, approval gate foundation |
| v0.4.0 | Released | Approved patch apply: recorded approval token, 허용된 verification command, guarded patch apply |
| v0.5.0 | Released | Read-only TUI beta: terminal surface foundation |
| v0.6.0 | Released | TUI approval과 diff view |
| v0.7.0 | Released | TUI session transcript view |
| v0.8.0 | Released | TUI evidence와 stop-gate view |
| v0.9.0 | Released | Backend resource sampling: sidecar CPU/RSS/memory/disk sampling, local ledger/SQLite 기록, CLI status field |
| v0.10.0 | Released | TUI resource monitor: CPU, memory, latency, token throughput, resource-pressure status |
| v0.11.0 | Released | Backend chat resource governor: backend health/resource threshold, critical-pressure 차단, degraded-pressure max-token clamp, CLI/ledger reporting |
| v0.12.0 | Released | Read-only team admission preview: 최신 resource sample, parallel admission, sequential fallback, dispatch 차단 |
| v0.13.0 | Released | Team admission gate: requested lane enforcement, sequential fallback, critical-pressure 차단, ledger 기록 |
| v0.14.0 | Released | Team policy preflight: 요청 write path와 command를 dispatch 전에 검사하고 ask/deny는 worker launch 차단 |
| v0.15.0 | Released | Team file ownership preflight: lane별 write path를 정규화하고 cross-lane conflict는 worker launch 차단 |
| v0.16.0 | Released | Team approval queue integration: policy/ownership block이 approval request record를 쓰고 `tui approvals`에 표시됨 |
| v0.17.0 | Released | Team context and model governor: 요청 context clamp, resource-sensitive model route hint, ledger 기록 |
| v0.18.0 | Released | Performance baseline report: p50/p95 latency, tokens/sec, context clamp count, peak RSS, pressure state, backend/model/session grouping |
| v0.19.0 | Released | Benchmark harness foundation: fixture schema 검증, benchmark run ledger/projection, reproducibility metadata, redacted local report export |
| v0.20.0 | Released | Executable benchmark runner: active backend sidecar로 prompt artifact를 실행하고 local 0-3 score, `measured-locally` benchmark row, model/token/resource metric linkage 기록 |
| v0.20.1 | Released | Benchmark evidence status: 실제 Qwen executable smoke 측정 문서화, `model eval-plan`이 최신 local measured benchmark row를 표시 |
| v0.21.0 | Released | Benchmark-driven optimization policy: `monitor optimize`가 측정된 local metric과 benchmark evidence로 context budget, lane count, fallback, model route 추천 |
| v0.22.0 | Released | Dispatcher hardening: `team dispatch`가 dispatch-time file ownership을 강제하고 failed-worker continuation을 기록하며 최신 team runtime status를 표시 |
| v0.23.0 | Planned | 공식 binary download foundation: release build pipeline, GitHub Release asset, macOS Apple Silicon과 Windows x86_64 `rpotato` artifact, binary checksum, `rpotato doctor` release smoke |
| v0.24.0 | Planned | Cross-platform release hardening: macOS Intel artifact, checksum publication, Windows keep-cache/purge-cache uninstall smoke, release notes template |
| v0.25.0 | Planned | Verified model install path: local evidence 후 source-backed candidate 승격, registry registration, install download flow, RAM/mmproj evidence gate |
| v0.26.0 | Planned | Ontology runtime store: canonical internal typed graph/store, compact context view, source-pointer reread rule, import/export inspection view |
| v0.27.0 | Planned | Plugin adapter hardening: Codex local-directory import completion, Claude Code adapter follow-up, external command/MCP/background permission 기본 차단 |
| v0.28.0+ | Planned | Post-MVP packaging decision: Homebrew/Scoop/winget 결정, Linux x86_64와 Linux ARM64 artifact, optional local HTML report/dashboard |
