# 로드맵

이 로드맵은 버전 전용으로 관리합니다. 새 로드맵 항목은 반드시 구체적인
버전 행으로 추가합니다.

`v0.29.0`-`v0.41.0` train의 실행 순서, release cycle, 건너뛸 수 없는 gate는
[release-train.md](release-train.md)에 정의합니다. v0.29.1에도 유지되는 v0.29.0의
release 차단 기반 보정 기록은 [v0.29-correction-plan.md](v0.29-correction-plan.md)에
남겨 둡니다.

`rolling-potato`는 작은 로컬 모델용 coding-agent runtime입니다. Runtime은
instruction, permission, state, ledger, transcript, evidence, monitoring,
ontology 기반 context, plugin, hook, subagent, team, TUI surface를 소유합니다.
공개 모델 claim과 benchmark claim은 기록된 evidence를 넘어가면 안 됩니다.

## 상태 기준

- `Released`: 해당 버전이 release contract를 완료했습니다.
- `Superseded`: 불완전하거나 결함이 있던 publication을 이후 patch/recovery
  release가 대체했습니다.
- `Consolidated`: 별도 공개 릴리즈 대신 이후 exact-tree release에 포함된 내부
  milestone입니다.

## 릴리즈 기록

### 런타임 기반 (`v0.1.0`-`v0.8.0`)

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

### 관측성과 팀 기반 (`v0.9.0`-`v0.22.0`)

| Version | 상태 | 범위 |
| --- | --- | --- |
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

### 배포, 모델, 온톨로지 (`v0.23.0`-`v0.30.0`)

| Version | 상태 | 범위 |
| --- | --- | --- |
| v0.23.0 | Released | 공식 binary download foundation: macOS Apple Silicon과 Windows x86_64용 GitHub Release binary workflow, release asset checksum, `rpotato doctor` binary smoke |
| v0.23.1 | Released | Windows binary link fix: `rusqlite`가 bundled Windows SQLite linkage를 사용해 release asset이 runner-provided `sqlite3.lib`에 의존하지 않음 |
| v0.24.0 | Released | Cross-platform release hardening: macOS Intel artifact, aggregate checksum publication, Windows keep-cache/purge-cache uninstall smoke, release notes template |
| v0.24.1 | Released | Checksum workflow fix: basename-only `.sha256` path, aggregate release upload repository context, checksum path smoke guard |
| v0.24.2 | Released | Aggregate checksum checkout fix: checksum job이 repo-local checksum guard 실행 전 repo를 checkout |
| v0.25.0 | Released | Verified model install gate: source-backed candidate는 registry 등록 전 local promotion evidence, artifact checksum/size, backend smoke ledger, RAM/mmproj evidence, measured benchmark row가 필요 |
| v0.26.0 | Released | Ontology runtime store: project-local canonical typed graph JSONL, Layer A seed, compact context view, source-pointer reread rule, import/export inspection view |
| v0.27.0 | Released | Plugin adapter hardening: Codex/Claude Code local-directory import가 source manifest/snapshot hash를 기록하고 capability를 mapping하며 shell/MCP/background/runtime-setting permission을 기본 차단하고 source drift 시 validate/enable을 차단 |
| v0.28.0 | Superseded | Linux release artifact matrix를 추가했지만 첫 publication이 GitHub ARM runner 종료로 끊겨 Linux asset과 aggregate checksum이 완료되지 않음 |
| v0.28.1 | Superseded | Release test gate를 추가했지만 Linux test execution이 signal 143으로 종료되어 artifact publication 전 실패 |
| v0.28.2 | Superseded | Sidecar timeout cleanup을 추가했지만 release test gate가 GitHub runner shutdown signal로 artifact publication 전 실패 |
| v0.28.3 | Superseded | Serialized release test gate도 sidecar timeout fixture 이후 GitHub runner shutdown으로 artifact publication 전 실패 |
| v0.28.4 | Superseded | Release-runner-safe skip gate도 stale-record test에서 Unix PID wrap hazard가 드러나 실패 |
| v0.28.5 | Released | 완성된 Linux/macOS/Windows release artifact: stale sidecar record용 Unix PID guard, full serialized release gate 복구, target별 build/smoke/package job, Linux x86_64/Linux ARM64 tarball, aggregate checksum publication |
| v0.29.0 | Superseded | 지속 가능한 single-agent runtime 보정을 출시했지만 Windows `.sha256` CRLF 때문에 aggregate checksum의 Unix `shasum -c` 검증이 실패 |
| v0.29.1 | Released | 크로스 플랫폼 aggregate checksum 수정: 명시적 ASCII/LF Windows checksum 출력과 LF/CRLF regression guard를 추가하고 v0.29.0 runtime 보정을 유지 |
| v0.30.0 | Released | 검증된 모델 도입: Qwen/Gemma pinned artifact local 평가, canonical chat/benchmark/RAM/mmproj provenance gate, managed registry install, fail-closed 지속 기본 모델 선택, 파생 project ledger 복구를 model weight 번들 없이 출시 |

### 영속 런타임과 협업 (`v0.31.0`-`v0.38.0`)

| Version | 상태 | 범위 |
| --- | --- | --- |
| v0.31.0 | Superseded | Backend streaming/cancellation을 구현했지만 Windows artifact가 sidecar stop fallback에서 실패해 첫 release가 불완전하게 종료 |
| v0.31.1 | Released | v0.31.0 streaming/cancellation 동작을 유지하면서 Windows sidecar stop fallback과 5개 platform artifact 전체를 복구 |
| v0.32.0 | Released | 지속 가능한 대화 resume: canonical user/visible-model/tool/evidence transcript, ledger 순서를 보존하는 재생성 가능한 SQLite projection, 하나의 공유 bounded source-context budget, mutation 전 session 선택 preflight, 불확실한 side effect 재실행 없는 idempotent `resume`/`continue` |
| v0.32.1 | Released | 안정 도구 체계 갱신: Rust 1.97.0, 최신 안정 Cargo resolution, Node.js 24 GitHub Actions, 최신 GA hosted runner, 검증된 설치 provenance를 갖춘 source-pinned llama.cpp b9982 artifact |
| v0.33.0 | Released | 실행 가능한 hook과 skill: lifecycle hook과 built-in skill state machine이 deterministic ordering, fail-closed result, policy enforcement, evidence, stop criteria를 적용하는 영속 agent loop 안에서 실행됨 |
| v0.34.0 | Superseded | Runtime-owned Interactive TUI, pending action 승인/거부, diff·tool output 확인, workflow resume/cancel, recoverable exact prepared transaction을 구현했지만 binary publication이 일부만 완료됨 |
| v0.34.1 | Superseded | Windows file identity와 Linux ARM64 source recovery를 이식 가능하게 복구했지만 binary publication이 일부만 완료됨 |
| v0.34.2 | Superseded | Windows ConPTY lifecycle, long-path atomic replacement, bounded native fixture를 복구했지만 binary publication이 일부만 완료됨 |
| v0.34.3 | Released | Bounded platform sampling, status-line health probe, Rust fake sidecar와 graceful TCP half-close로 native release gate를 복구하고 검증된 exact 11-asset set을 게시 |
| v0.35.0 | Superseded | Bounded subagent source를 반영했지만 실제 release tag 환경이 일반 PR policy fixture로 누출되어 binary build 전에 release test gate가 중단됨 |
| v0.35.1 | Released | Ambient tag context를 지우는 hermetic release-contract fixture와 함께 v0.35 bounded subagent 구현 전체를 유지한 복구 릴리스 |
| v0.36.0 | Released | Team 실행: admitted lane dispatch, team stage 진행, action-time ownership enforcement, result reconciliation, failed lane 처리, completion 전 verification/stop gate 적용 |
| v0.37.0 | Released | Codex plugin execution adapter: local import된 지원 capability를 native policy/hook/skill boundary에서 실행하고 shell/MCP/background/remote/write capability는 명시적 승인 전까지 기본 차단 |
| v0.37.1 | Consolidated | 별도 발행하지 않은 architecture-foundation 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.2 | Consolidated | 별도 발행하지 않은 foundation/platform 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.3 | Consolidated | 별도 발행하지 않은 inference-boundary 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.4 | Consolidated | 별도 발행하지 않은 workflow-storage-compatibility 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.5 | Consolidated | 별도 발행하지 않은 validated-domain-view 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.6 | Consolidated | 별도 발행하지 않은 workflow-transaction/recovery 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.7 | Consolidated | 별도 발행하지 않은 observability-boundary 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.8 | Consolidated | 별도 발행하지 않은 knowledge/policy 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.9 | Consolidated | 별도 발행하지 않은 patch-boundary 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.10 | Consolidated | 별도 발행하지 않은 runtime/reporting 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.11 | Consolidated | 별도 발행하지 않은 extension-boundary 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.12 | Consolidated | 별도 발행하지 않은 collaboration-boundary 구현 milestone이며 exact-tree v0.37.13 release에 포함 |
| v0.37.13 | Superseded | 동작을 보존한 전체 architecture ownership migration source를 포함하지만 Unix 전용 source-install 함수를 Windows test binary가 import해 Windows archive를 게시하지 못함 |
| v0.37.14 | Released | Windows release 복구: source-install import를 platform gate로 제한하고 tag 전 Windows release preflight를 보강하면서 v0.37.x architecture migration 전체를 유지 |
| v0.38.0 | Released | Claude Code plugin 실행 adapter: canonical local skill/command를 native read-only contract로 실행하고 unsupported semantic을 명시하며 dynamic shell과 위험 capability를 default-deny로 유지 |

### 성능, 배포, 보고 (`v0.39.0`-`v0.41.0`)

| Version | 상태 | 범위 |
| --- | --- | --- |
| v0.39.0 | Released | 통합 성능 최적화: 완료된 agent/subagent/team workflow의 결정적 예산, 정상 event projection의 canonical full-ledger 재읽기 0회, 더 작은 bounded-worker request envelope, unsupported model claim 없는 exact-candidate regression gate |
| v0.40.0 | Released | 과거 package-manager 실험: manifest와 lifecycle 검증을 이 version에서 제공했지만 지원하지 않는 외부 channel과 저장소 자동화는 2026-07-19에 중단 |
| v0.41.0 | Released | Optional local static HTML monitoring report: 별도의 telemetry source of truth 없이 기존 SQLite/ledger data를 읽고 redaction, responsive rendering, external request 0건을 유지하는 self-contained local export |

## 배포 경계

이 저장소의 GitHub Releases만 지원하는 download·배포 channel입니다.
Homebrew, Scoop, winget 등 다른 package 저장소는 roadmap target이나 release
gate가 아닙니다. v0.40.0 package-manager 작업은 과거 release evidence로만
남기며 generator, fixture, workflow는 2026-07-19에 제거했습니다.
