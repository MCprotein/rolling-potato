# v0.29.0-v0.41.0 릴리즈 트레인

이 문서는 [ROADMAP.md](../../ROADMAP.md)의 `v0.29.0`부터 `v0.41.0`까지를
버전 하나당 Codex Goal 하나로 구현하고 출시하는 실행 계약입니다. 이 release
train에는 Ultragoal을 사용하지 않습니다.

## 목표

작은 로컬 모델을 위한 local-first coding-agent runtime으로서 Claude
Code/Codex 대체 경로를 완성합니다. 한 번에 minor version 하나씩 release
cycle을 실행합니다. 버전을 건너뛰거나 측정하지 않은 모델 결과를
확정하거나 runtime correctness보다 packaging 편의 기능을 앞세우면 안
됩니다.

각 버전은 별도 Goal을 사용하며 tag, GitHub Release, 필수 asset, checksum,
workflow gate, branch cleanup, review evidence를 모두 검증한 뒤에만
완료합니다. 다음 버전은 새 Goal로 시작합니다.

## 필수 버전 사이클

모든 버전은 다음 순서를 완료해야 합니다.

1. 최신 `main`에서 `release/vX.Y.Z`를 생성합니다.
2. 로드맵과 이 문서를 기준으로 버전 목표와 acceptance evidence를
   확인합니다.
3. 테스트를 포함한 가장 작은 완성 범위를 구현합니다.
4. 영문 문서와 한국어 문서를 함께 수정합니다.
5. Targeted test, `cargo fmt --check`, `cargo test --locked`,
   `cargo clippy --all-targets -- -D warnings`, release build, 관련 CLI smoke
   test를 실행합니다.
6. 독립 review를 실행하고 blocking finding을 모두 해결합니다.
7. Conventional Commit으로 release branch를 commit/push합니다.
8. `scripts/release/verify-release-policy.sh`와 release checklist를
   실행합니다.
9. `main`에 merge하고 merged commit을 재검증한 뒤 `vX.Y.Z` tag와 GitHub
   Release를 발행합니다.
10. GitHub Actions 성공, 모든 필수 platform archive, asset별 checksum,
    aggregate checksum, packaged-binary smoke 결과를 확인합니다.
11. Merge된 local/remote release branch를 제거하고 구체적인 evidence로
    현재 버전 Goal을 완료한 뒤 다음 버전 Goal을 시작합니다.

발행된 tag가 release asset gate를 충족하지 못하면 가장 작은 patch recovery
release를 만듭니다. 실패 tag는 evidence를 기록한 경우에만 Superseded로
표시하고 recovery release가 완료되기 전에는 다음 minor version을 시작하지
않습니다.

## 버전별 완료 증거

| 버전 | 필수 완료 증거 |
| --- | --- |
| `v0.29.0` | 영속 typed action, ontology 기반 source reread, 분리된 patch/verification 승인, guarded apply, verification 해석, canonical ledger 권위, stop gate, evidence, guarded 한국어 최종 보고를 포함하고 재시작 후에도 이어지는 `run` workflow |
| `v0.30.0` | 출처 기반 candidate 평가, 실제 local backend/RAM/mmproj/benchmark 기록, install-gate 결정, gate를 통과한 candidate만 사용하는 managed install/default flow |
| `v0.31.0` | Streaming response와 별도 process cancellation test, 1-300,000ms total timeout, request 전송 후 retry 0회, 중단 뒤 stale generation lease와 종료된 sidecar 0건, incomplete usage를 unknown으로 유지한 lifecycle/resource/model-run evidence |
| `v0.32.0` | 선택한 session을 resume하고 durable transcript/source pointer에서 bounded context를 재구성해 중단 workflow를 idempotent하게 계속하는 process-restart test |
| `v0.33.0` | Policy, evidence, stop criteria 우회가 불가능함을 증명하는 hook ordering/fail-closed fixture와 executable built-in skill state-machine test |
| `v0.34.0` | Runtime-owned state를 통한 approve/deny, diff/tool output, resume/cancel, monitoring/session operation interactive terminal test |
| `v0.35.0` | Bounded subagent launch, scoped context/tool/write/resource enforcement, structured result, failure handling, parent evidence merge test |
| `v0.36.0` | Parallel/sequential mode, action-time ownership, failed lane, resource pressure, verification, stop gate를 포함한 실제 team lane dispatch/stage/reconciliation test |
| `v0.37.0` | Native adapter를 통한 Codex local plugin capability execution test, 명시적 승인 전 risky capability 차단, marketplace/remote source path 부재 |
| `v0.37.1` | 영문/한국어 코드 아키텍처 계약, 재귀적으로 완전한 file-and-slice migration ledger, private compile-connected skeleton, dependency contract test, exact-head read-only candidate CI |
| `v0.37.2`-`v0.37.13+` | 각 ledger slice의 targeted behavior lock과 migration evidence, byte/order/recovery/security 호환성, 안정화된 patch별 bounded review 1회, v0.38.0 전 compatibility facade 0건 |
| `v0.38.0` | Claude Code local plugin mapping/conformance test, unsupported semantic 명시적 보고, 동일한 default-deny boundary |
| `v0.39.0` | 실제 agent/subagent/team CPU, RSS, context, token, latency, throughput 측정 evidence와 확인된 failure용 regression fixture |
| `v0.40.0` | GitHub Release checksum을 기준으로 채택한 Homebrew/Scoop/winget channel의 최신 공식 package format 검증과 clean install/upgrade/uninstall test |
| `v0.41.0` | SQLite/ledger parity, redaction, external telemetry 0건, 두 번째 source of truth 부재, desktop/mobile browser usability를 증명하는 local-only HTML export/server test |

## 건너뛸 수 없는 Gate

### 모델과 Benchmark Evidence

- Model candidate는 source, license, artifact URL, checksum, size, backend
  compatibility, RAM fit, mmproj need, measured product benchmark evidence가
  기록되기 전까지 `unverified`입니다.
- Public benchmark 비교는 같은 dataset/version, prompt/template, backend,
  quantization, context, sampling, scoring 조건이 필요합니다. 다르면
  non-comparable로 표시합니다.
- Score, capability, default model, hardware requirement를 상상해서 기록하지
  않습니다.
- 필요한 local hardware, storage, network, upstream artifact가 없으면
  blocker를 checkpoint합니다. 추론한 결과로 대체하지 않습니다.

### Runtime Safety와 영속성

- Model output은 tool을 직접 실행하지 않습니다.
- File write, command, download, plugin capability, subagent, team은 runtime
  policy와 필요한 명시적 승인을 통과해야 합니다.
- Pending approval, action, evidence, resume state는 process restart 뒤에도
  유지되어야 workflow를 완료로 판단할 수 있습니다.
- Unknown, corrupt, stale, conflicting state는 fail-closed 처리하고 validation
  gap을 기록합니다.
- Hook, skill, plugin, TUI, subagent, team, benchmark, HTML surface는 별도의
  policy/state/telemetry/stop-gate authority를 만들 수 없습니다.

### Plugin Boundary

- Plugin import는 local-directory only입니다. Marketplace, registry, catalog,
  mirror, remote URL integration은 범위 밖입니다.
- Shell, MCP, background process, remote connector, runtime-setting, file write
  capability는 기본 차단합니다.
- Codex compatibility를 Claude Code compatibility보다 먼저 구현합니다.
- Unsupported foreign semantic은 보고하며 더 넓은 permission으로 조용히
  흉내 내지 않습니다.

### Release와 Distribution

- Local test 성공이나 tag 생성만으로 버전을 완료로 판단하지 않습니다.
  Release workflow와 필수 platform asset이 성공해야 합니다.
- Evidence-backed roadmap 변경이 승인되지 않는 한 macOS Apple Silicon,
  macOS Intel, Linux x86_64, Linux ARM64, Windows x86_64를 필수 target으로
  유지합니다.
- Package manager 구현 시점에 최신 공식 specification을 확인합니다.
- Model weight, local state, log, secret, `.omx/`, `.rpotato/`, build output은
  commit이나 release archive에 포함하지 않습니다.

### 문서와 Claim

- User-facing behavior 또는 architecture contract가 바뀌면 영문 base 문서와
  한국어 번역을 함께 수정합니다.
- Model, license, performance, compatibility, legal claim은 명시적 출처가
  필요합니다. 해결되지 않은 claim은 `unverified`로 유지합니다.
- Release note는 측정된 behavior와 알려진 boundary를 기록하며 완료되지
  않은 replacement capability를 완성된 것처럼 표현하지 않습니다.

## Blocker와 Steering 규칙

필수 gate가 실패하면 다음 버전으로 넘어가지 않습니다. 다음 상황에서는
현재 story를 active로 유지하거나 blocker story를 기록합니다.

- 누락되거나 충돌하는 model/license/artifact evidence
- 실제 측정에 필요한 hardware, disk, network 부족
- Test, 독립 review, architecture invariant, security boundary 실패
- GitHub Actions 미완료 또는 release asset 누락/손상
- 구현을 무효화하는 upstream/package specification 변경
- 사용자 권한이 없는 destructive, credential-gated, license-changing,
  materially scope-changing 결정

버전별 Goal 실행은 evidence가 있을 때 work split, blocker-resolution task
추가, pending work 재정렬, pending wording 수정만 허용합니다. Gate 완화,
완료 조작, 조용한 version skip, product objective 변경은 금지합니다.

## 최종 Quality Gate

버전 Goal을 완료하기 전에 다음을 실행합니다.

1. 해당 버전 tag, GitHub Release, workflow, asset, checksum, branch cleanup을 검증합니다.
2. Changed-file cleanup과 consistency pass를 실행합니다.
3. 전체 verification suite를 다시 실행합니다.
4. 이 문서, `PLAN.md`, `ROADMAP.md`, runtime architecture 문서에서 도출한
   invariant를 증명합니다.
5. 독립 `code-reviewer` APPROVE를 확보합니다. Architecture를 변경하는 버전과
   최종 `v0.41.0` train closeout에는 추가 `architect` review가 필요합니다.
6. 최종 quality evidence를 기록한 뒤에만 해당 버전 Codex Goal을 완료합니다.
