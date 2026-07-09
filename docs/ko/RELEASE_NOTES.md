# 릴리즈 노트

## v0.20.1 - Benchmark Evidence Status

릴리즈 날짜: 2026-07-09

이 패치 릴리즈는 첫 Qwen executable smoke 실측 결과를 기록하고, model
evaluation preflight가 로컬 benchmark 측정 row를 반영하도록 수정합니다.

### 수정

- `rpotato model eval-plan qwen3.5-4b`가 더 이상 항상 `local benchmark
  status: not-run`을 표시하지 않습니다. SQLite `benchmark_runs` projection의
  최신 local `measured-locally` benchmark row를 표시합니다.
- 후보 artifact model id에 측정 row가 있으면 status가 `local-smoke-measured`로
  올라갑니다.

### 기록된 증거

- Qwen3.5-4B Q4_K_M local artifact는 이미 존재했고 SHA-256 검증을 통과했습니다.
- Managed `llama.cpp` version `9878 (2da668617)`가 `--ctx-size 4096`으로 Qwen
  sidecar를 시작했습니다.
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json
  --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32`는 score
  `3/3`, `local_pass=true`, latency `243ms`, total tokens `83`, resource
  pressure `normal`, peak RSS `3351363584` bytes를 기록했습니다.
- 측정 후 sidecar를 중지했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (186 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `rpotato model eval-plan qwen3.5-4b`
- `rpotato backend status`

### 경계

이 결과는 local smoke benchmark일 뿐입니다. Qwen3.5-4B를 `verified`로
승격하지 않으며 public benchmark parity도 주장하지 않습니다.

## v0.20.0 - Executable Benchmark Runner

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 executable local benchmark runner를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
  [--max-tokens <tokens>]` 명령.
- `benchmark run`은 실행 중인 backend sidecar를 호출하고 local
  `claim_state=measured-locally` benchmark row를 기록합니다.
- Expected/forbidden response marker, abstention requirement, 비어 있지 않은 model
  output을 기준으로 deterministic 0-3 local product score를 산정합니다.
- SQLite migration v4가 `benchmark_runs`에 `model_run_id`, prompt artifact
  checksum/length, local pass flag, marker count, latency, token count, resource
  pressure, peak RSS field를 추가합니다.
- `benchmark report --format jsonl`이 executable benchmark field를 export합니다.
- `benchmarks/fixtures/executable-smoke.json`과
  `benchmarks/prompts/executable-smoke.txt`가 첫 executable smoke fixture/prompt
  pair를 제공합니다.
- Executable benchmark boundary, redaction, observability linkage에 대한
  영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (185 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark validate benchmarks/fixtures/executable-smoke.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` 실행 중인 sidecar가 없을 때 fail-closed
- `rpotato benchmark report --format jsonl`

### 알려진 제한

- `benchmark run`은 이미 실행 중인 backend sidecar와 `rpotato backend start`로
  시작한 local model file이 필요합니다. 이 릴리즈는 model weight를 bundle하거나
  자동 선택하지 않습니다.
- Executable runner는 local product score만 기록합니다. Public benchmark score와
  비교하거나 leaderboard parity를 주장하지 않습니다.
- Source-read compliance와 hallucination scoring은 아직 marker/proxy 기반입니다.
  Tool/evidence-aware scoring은 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.19.0 - Benchmark Harness Foundation

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 metadata-only benchmark harness surface를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato benchmark validate <fixture.json>` 명령.
- 새 `rpotato benchmark record --fixture <fixture.json>` 명령.
- 새 `rpotato benchmark report --format jsonl` 명령.
- Project-local fixture schema 검증: runtime capability, model/runtime responsibility,
  expected route, policy decision, escalation target, required tool/source/evidence
  record, abstention requirement, ontology view, context budget, backend/model artifact
  identifier, sampling policy, raw artifact retention policy.
- SQLite migration v3가 `benchmark_runs`에 session, fixture checksum, claim state,
  reproducibility manifest, redacted report field를 추가합니다.
- Metadata-only benchmark record는 `claim_state=not-comparable`, `score=null`을
  사용합니다. Model 실행이나 public benchmark parity claim은 하지 않습니다.
- `benchmarks/fixtures/sample.json`은 raw prompt/source가 없는 CLI-contract smoke
  fixture입니다.
- Benchmark fixture contract, observability integration, v0.19.0 rollout status에 대한
  영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (183 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark report --format jsonl`
- `rpotato monitor status`

### 알려진 제한

- Benchmark 명령은 model 실행, fixture score 산정, public benchmark와 local score 비교를
  하지 않습니다.
- Hardware/RAM/power/thermal manifest field는 executable benchmark run이 수집하기 전까지
  `not-recorded` placeholder입니다.
- Fixture suite, ontology-view scoring, public benchmark parity report,
  benchmark-driven optimization policy는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.18.0 - Performance Baseline Report

릴리즈 날짜: 2026-07-08

이 릴리즈는 read-only local performance baseline report를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato monitor baseline` 명령.
- 새 raw prompt/source store를 추가하지 않고 기존 local ledger/SQLite projection
  metric을 집계합니다.
- p50/p95 latency, average tokens/sec, context clamp count, context tokens
  dropped, peak RSS, pressure-state distribution, model/backend/session grouping을
  보고합니다.
- 이 report는 local metric evidence일 뿐이며, model artifact를 선택하거나
  source-backed model capability claim을 만들지 않습니다.
- v0.18.0 performance baseline 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (172 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`

### 알려진 제한

- `monitor baseline`은 local projection에 이미 있는 metric만 보고합니다.
  Benchmark를 실행하거나 continuous background sample을 수집하지 않습니다.
- Benchmark harness recording, redacted report export, benchmark-driven
  optimization policy는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.17.0 - Team Context And Model Governor

릴리즈 날짜: 2026-07-08

이 릴리즈는 첫 team context/model governor preflight를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team governor --lanes <count> --context-tokens <tokens>` 명령.
- 명시적인 runtime policy simulation을 위한 선택 옵션
  `--context-limit <tokens>`와 `--model-tier small|standard|large`.
- 최신 resource sample을 사용한 admitted-lane 및 context/model governor decision.
- 설정 budget, degraded-pressure budget, local small-model soft budget에 맞춘
  effective context-token clamp.
- Local model route hint: `keep`, `downgrade`, `escalate`, `defer`.
- Team governor decision의 ledger/SQLite 기록.
- v0.17.0 governor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (170 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard`
- `rpotato team governor --lanes 2 --context-tokens 1024 --context-limit 4096 --model-tier small`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, normal pressure에서는
clamped context/model decision이 기록되고 critical pressure에서는 `defer` route hint로
차단되는지 확인합니다.

### 알려진 제한

- `team governor`는 preflight/reporting surface입니다. Worker를 시작하거나 실제 model
  artifact를 선택하거나 model routing을 실행하지 않습니다.
- Model route hint는 local runtime policy hint일 뿐이며, 실제 model artifact capability에
  대한 source-backed claim이 아닙니다.
- Dispatch-time ownership enforcement와 failed-worker continuation은 후속 범위입니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.16.0 - Team Approval Queue Integration

릴리즈 날짜: 2026-07-08

이 릴리즈는 차단된 team admission decision을 read-only approval queue에 연결합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `.rpotato/approval-requests/` 아래 project-local approval request store를 추가했습니다.
- Blocking `team admit` policy/ownership decision은 team admission ledger event에 연결된
  redacted approval request record를 씁니다.
- `rpotato tui approvals`는 patch proposal approval 옆에 team admission approval request를
  표시합니다.
- `rpotato init`은 project runtime layout 일부로 approval request directory를 생성합니다.
- Team admission 출력은 policy 또는 ownership decision 검토가 필요할 때 approval request id와
  path를 포함합니다.
- v0.16.0 approval queue integration 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (165 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato tui approvals`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, policy/ownership으로
차단된 team admission record가 read-only TUI approval queue에 표시되는지 확인합니다.

### 알려진 제한

- `tui approvals`는 read-only입니다. Team admission request를 나열하지만 approve, deny,
  dispatch resume은 수행하지 않습니다.
- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, 실제 worker
  execution 중 ownership enforcement를 수행하지 않습니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.15.0 - Team File Ownership Preflight

릴리즈 날짜: 2026-07-08

이 릴리즈는 enforced team admission gate에 file ownership preflight를 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato team admit --lanes <count>`가 반복 가능한
  `--write-owner <lane:path>` ownership claim을 받습니다.
- Ownership path는 dispatch 전에 정규화됩니다. 예를 들어 `README.md`와
  `./README.md`는 같은 ownership key로 판정됩니다.
- Cross-lane ownership conflict는 향후 worker launch 이전 단계에서 admission을
  차단합니다.
- Owned write path도 기존 write policy preflight에 포함되므로 approval-required write는
  approval queue integration이 생기기 전까지 계속 dispatch를 차단합니다.
- Team admission 출력과 ledger event detail에 ownership claim count, ownership status,
  ownership blocked flag, owned write path, per-claim decision을 포함합니다.
- v0.15.0 ownership preflight 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (163 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, 서로 다른 lane-owned
path는 allocation으로 표시되고 정규화된 cross-lane ownership conflict는 worker launch 전에
dispatch를 차단하는지 확인합니다.

### 알려진 제한

- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, 실제 worker
  execution 중 ownership enforcement를 수행하지 않습니다.
- Approval queue integration이 아직 후속 범위라서 `ask` decision은 dispatch를 차단합니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.14.0 - Team Policy Preflight

릴리즈 날짜: 2026-07-08

이 릴리즈는 enforced team admission gate에 policy preflight를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato team admit --lanes <count>`가 반복 가능한 `--write <path>`와
  `--command <command>` preflight check를 받습니다.
- 요청 write path는 `policy check-path --write`와 같은 policy engine으로 분류합니다.
- 요청 command는 `policy check-command`와 같은 policy engine으로 분류합니다.
- `allow` policy check는 admission gate를 통과할 수 있습니다.
- `ask`와 `deny` policy check는 향후 worker launch 이전 단계에서 dispatch를 차단합니다.
- Team admission 출력과 ledger event detail에 policy check count, policy status,
  policy blocked flag, requested write, redacted command, per-check decision을
  포함합니다.
- v0.14.0 policy preflight 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (159 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, command preflight는
통과하고 write preflight는 worker launch 전에 `approval-required`로 차단되는지
확인합니다.

### 알려진 제한

- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, file ownership
  allocation을 수행하지 않습니다.
- Approval queue integration이 아직 후속 범위라서 `ask` decision은 dispatch를 차단합니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.13.0 - Team Admission Gate

릴리즈 날짜: 2026-07-07

이 릴리즈는 v0.12.0의 read-only team admission preview를 첫 enforced admission
gate로 바꿉니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team admit --lanes <count>` 명령.
- Admission decision을 append-only ledger와 SQLite projection에 기록합니다.
- Normal pressure에서는 요청한 parallel lane을 허용합니다.
- Missing/unknown 또는 degraded pressure에서는 sequential lane 하나로 fallback합니다.
- Critical pressure에서는 향후 worker launch 이전 단계에서 blocked error를 반환합니다.
- `team status`는 read-only로 남고, `team admit`은 mutating gate가 됩니다.
- v0.13.0 admission gate 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (157 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, resource sample이
없을 때 `team admit`이 ledger event를 기록하면서 sequential lane 하나로 fallback하는지
확인합니다.

### 알려진 제한

- 요청 write와 command에 대한 policy preflight는 v0.14.0에서 도입됐습니다. Full worker
  dispatch와 file ownership allocation은 후속 범위입니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp, file ownership, tool risk, approval queue, model
  downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.12.0 - Team Admission Preview

릴리즈 날짜: 2026-07-07

이 릴리즈는 resource monitoring/governor 작업 위에 첫 read-only team admission
surface를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team status` 명령.
- 향후 subagent/team dispatch가 재사용할 resource lane admission policy.
- Normal pressure에서는 요청한 parallel lane을 허용합니다.
- Missing/unknown 또는 degraded pressure에서는 sequential lane 하나로 fallback합니다.
- Critical pressure에서는 새 team dispatch를 차단합니다.
- `team status`가 최신 resource sample metadata, requested lane, admitted lane,
  admission, dispatch-blocked flag, fallback, reason, hint, read-only boundary를
  표시합니다.
- v0.12.0 team admission preview 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (153 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, resource sample이
없을 때 `team status`가 workflow state를 변경하지 않고 sequential fallback을
보고하는지 확인합니다.

### 알려진 제한

- `team status`는 admission preview일 뿐입니다. Subagent 시작, team lane dispatch,
  workflow mutation, file ownership enforcement는 아직 수행하지 않습니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Enforced resource admission gate는 v0.13.0에서 도입됐고, 남은 dispatcher policy는
  후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.11.0 - Backend Chat Resource Governor

릴리즈 날짜: 2026-07-07

이 릴리즈는 managed backend sidecar를 위한 첫 runtime resource governor slice를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato backend chat`이 model 실행 전에 backend CPU/RSS/disk resource pressure를
  sampling합니다.
- Critical resource pressure에서는 `/v1/chat/completions` 요청을 보내기 전에 chat을
  차단합니다.
- Degraded resource pressure에서는 effective max-token budget을 clamp하고,
  normal/unknown pressure 요청은 유지합니다.
- `backend chat`과 `run` 출력이 requested max tokens와 effective max tokens를
  구분하고 governor admission/token action을 표시합니다.
- Redacted ledger event가 raw prompt/response 없이 governor admission, token action,
  reason, sample event id를 기록합니다.
- v0.11.0 governor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (149 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato backend chat --prompt smoke --max-tokens 256`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용합니다. 실행 중인
backend sidecar가 없을 때 `backend chat`은 model 실행 전에 fail closed 해야 하며
raw prompt/response storage를 만들면 안 됩니다.

### 알려진 제한

- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- v0.11.0 governor는 backend chat에만 적용됩니다. Team admission preview와
  sequential fallback은 v0.12.0에서 도입되며, 실제 subagent/team dispatch admission은
  후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.10.0 - TUI Resource Monitor

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 managed backend sidecar resource-pressure
monitor를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui monitor`가 resource sample count, 최신 pressure status, CPU
  percent, average/peak RSS, disk bytes, recorded timestamp를 보여줍니다.
- Model monitoring summary가 total token과 average latency에 더해 average tokens
  per second를 표시합니다.
- Monitor layout은 dependency-free terminal-safe surface로 유지되며,
  `COLUMNS=64` 같은 좁은 terminal render도 다룹니다.
- v0.10.0 TUI monitor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (148 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui monitor`
- `COLUMNS=64 rpotato tui monitor`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
observability schema v2 상태에서 monitor view가 resource pressure, resource sample
count, model/token count, read-only action, beta boundary를 workflow mutation 없이
렌더링하는지 확인했습니다.

### 알려진 제한

- Resource monitor data는 event-driven이며 최신 recorded sample을 보여줍니다.
  Continuous live polling은 아닙니다.
- Runtime resource governor 동작은 v0.10.0에 포함되지 않으며, 첫 backend chat
  governor slice는 v0.11.0에서 도입됩니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.9.0 - Backend Resource Sampling

릴리즈 날짜: 2026-07-07

이 릴리즈는 관리형 `llama.cpp` sidecar를 위한 첫 backend resource monitoring
slice를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- CPU percent, average/peak RSS bytes, disk bytes, sample count, pressure
  status, recorded timestamp를 담는 `resource_samples` SQLite projection schema.
- `backend start`, already-running start 재사용, 실행 중인 sidecar의 `backend
  status`, `backend chat`에서 backend resource sampling.
- Redacted `backend.resource.sampled` ledger event. Raw prompt, response, source
  text는 기본적으로 계속 저장하지 않습니다.
- `monitor status`가 resource sample count와 최신 CPU/RSS/disk/pressure field를
  보여줍니다.
- `monitor prune --dry-run`이 resource sample row count를 포함합니다.
- v0.9.0 monitoring 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (147 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato monitor status`
- `rpotato backend status`
- `rpotato monitor prune --before 30d --dry-run`

CLI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
observability schema v2와 monitor 출력의 resource sample count, latest resource
CPU/RSS/disk/pressure field를 확인했습니다.

### 알려진 제한

- Resource sampling은 event-driven이며 continuous background polling은 아닙니다.
- TUI resource-pressure 표시는 v0.9.0에 포함되지 않으며, v0.10.0에서 도입되었습니다.
- Runtime resource governor 동작은 v0.9.0에 포함되지 않으며, 첫 backend chat
  governor slice는 v0.11.0에서 도입됩니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.8.0 - TUI Evidence And Stop Gate View

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 evidence/stop-gate status inspection을 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui evidence`는 runtime evidence store path, runtime evidence record
  count, project evidence artifact count, SQLite evidence record count, SQLite
  stop-gate result count, stale evidence policy summary를 보여줍니다.
- TUI overview가 evidence view를 안내합니다.
- `monitor status`가 SQLite evidence/stop-gate result count를 함께 보여줍니다.
- Project-local artifact를 세는 read-only evidence store status API.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (143 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui evidence`
- `COLUMNS=64 rpotato tui evidence`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
evidence view가 runtime evidence, project evidence, observability, stop-gate count,
stale policy, validation command, read-only beta boundary field를 렌더링하는지 확인했습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Evidence view는 evidence/stop-gate status만 보고하며, workflow를 pass/fail 판정하지 않습니다.
- Terminal stop-gate evaluation, tool output viewer, subagent/team status, plugin
  permission review는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.7.0 - TUI Session Transcript View

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 선택한 session의 event inspection을 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui transcript <session-id>`는 선택한 session metadata와 timestamp 순
  event timeline을 보여줍니다.
- `rpotato tui sessions`가 transcript inspection command를 안내합니다.
- Session event를 읽는 SQLite observability read API.
- Transcript replay, resume, cancellation, workflow mutation을 TUI beta 밖에 두는
  read-only boundary.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (140 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato session new`
- `rpotato state resume`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `COLUMNS=64 rpotato tui transcript <session-id>`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 새 session을 만들고 no-op
resume event를 기록한 뒤, session list와 transcript timeline에 2개의 projected ledger
event가 보이는지 확인했습니다. Raw model transcript replay나 workflow mutation은
수행하지 않았습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Transcript view는 projected ledger event metadata와 summary만 보여줍니다. Raw event
  detail과 model transcript replay는 후속 agent-loop 작업입니다.
- Tool output viewer, subagent/team status, plugin permission review,
  stop-gate evidence view는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.6.0 - TUI Approval And Diff Views

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 patch approval queue와 diff inspection view를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui approvals`는 project-local patch proposal record를 나열합니다.
- `rpotato tui diff <proposal-id>`는 proposal metadata, approve/dry-run command
  hint, 저장된 unified diff를 보여줍니다.
- Patch proposal summary/detail을 읽는 read-only API.
- TUI에서 `---`, `+++`, `@@`, `-`, `+` diff line이 유지되도록 literal diff rendering.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (138 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path src/lib.rs --find 1 --replace 2`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `COLUMNS=64 rpotato tui diff <proposal-id>`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 patch proposal을 만들고,
pending approval record와 저장된 unified diff를 표시했으며 patch approve나 apply는
수행하지 않았습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Approval queue와 diff view는 기존 patch proposal record를 inspect만 합니다.
  Approval과 apply는 여전히 `rpotato patch approve`로 수행합니다.
- Transcript view, tool output viewer, subagent/team status, plugin permission review,
  stop-gate evidence view는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.5.0 - Read-Only TUI Beta

릴리즈 날짜: 2026-07-07

이 릴리즈는 terminal-only 환경을 위한 첫 read-only TUI beta surface를 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui` overview dashboard
- `rpotato tui monitor` model/token monitoring view
- `rpotato tui sessions` full session id와 resume hint가 있는 session-history view
- SSH/Linux server 친화적인 dependency-free ASCII layout
- approval, patch apply, resume, cancel, workflow mutation을 수행하지 않는 read-only boundary
- TUI beta surface에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (133 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

TUI smoke는 project/session 상태, SQLite observability path, 기록된 model/token metric, session history, read-only beta boundary를 보여줬습니다.

### 알려진 제한

- TUI beta는 interactive event loop가 아니라 one-shot read-only render입니다.
- approval queue, diff viewer, transcript view, subagent/team status, plugin permission review, stop-gate evidence view는 후속 작업입니다.
- 첫 beta는 의도적으로 TUI framework dependency를 추가하지 않습니다. Interaction requirement가 안정된 뒤 더 풍부한 TUI crate가 필요한지 재검토할 수 있습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.4.0 - Approved Patch Apply

릴리즈 날짜: 2026-07-07

이 릴리즈는 patch approval surface를 dry-run gate 확인에서 승인된 patch apply,
rollback record, 선택적 verification command 실행까지 확장합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch approve <proposal-id> --token <token>`은 `--dry-run`이 없을 때 승인된 proposal을 적용합니다.
- apply 전 current file SHA-256 guard로 preview 이후 target file이 바뀐 stale proposal을 차단합니다.
- `.rpotato/patch-proposals/` 아래 rollback record를 생성합니다.
- write 이후 applied SHA-256을 검증합니다.
- `--verify-command <command>`는 apply 이후 allow 정책을 통과한 단순 argv verification command를 실행합니다.
- verification 실패 시 rollback을 시도하고 성공으로 보고하지 않습니다.
- 새 patch application 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (127 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke` scratch project smoke
- `rpotato patch preview --path README.md --find "Local coding agents for potato PCs." --replace "Local coding agents for potato PCs. Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --verify-command "rg Smoke README.md"`

Patch smoke는 `status: applied`, rollback record 생성, `verification status:
passed`, verification exit code `0`을 반환했습니다. Smoke는 repository working
tree가 아니라 `/private/tmp` project fixture에서 실행했습니다.

### 알려진 제한

- patch preview는 여전히 project-local UTF-8 text file 하나에 대한 명시적인 단일 find/replace proposal만 지원합니다.
- verification command는 policy가 allow한 단순 argv command로 제한됩니다. Shell syntax, quoting, pipe, redirect, environment expansion은 지원하지 않습니다.
- model action output은 아직 patch preview/apply에 자동 연결되지 않습니다.
- verification output interpretation과 final Korean task reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.3.0 - Patch Diff Approval Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 첫 patch diff display와 approval gate surface를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch preview --path <path> --find <text> --replace <text>`
- project-local text replacement 하나에 대한 unified diff rendering
- `.rpotato/patch-proposals/` 아래 project-local proposal record
- 생성된 proposal의 approval token 표시
- `rpotato patch approve <proposal-id> --token <token> --dry-run`
- patch 적용 없는 approval gate 검증과 ledger event 기록
- 새 patch 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (123 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path RELEASE_NOTES.md --find "Run Skeleton Preview" --replace "Run Skeleton Preview Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --dry-run`

Patch smoke는 `status: diff-ready`와 예상 unified diff를 반환했고, dry-run
approval에서는 `status: gate-passed`를 반환했습니다. Smoke 이후 target file에
Git diff가 없어 파일을 수정하지 않았음을 확인했습니다.

### 알려진 제한

- patch preview는 project-local UTF-8 text file 하나에 대해 명시적인 단일 find/replace proposal만 지원합니다.
- 이 릴리즈에서 patch approval은 dry-run 전용입니다. gate 결과를 기록하지만 patch를 적용하지 않습니다.
- model action에서 patch preview로 이어지는 agent-loop 통합은 후속 작업입니다.
- verification command execution, rollback handling, final Korean reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.2.0 - Run Skeleton Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 managed `llama.cpp` sidecar 위에 첫 `rpotato run` vertical slice를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- context-aware `rpotato run "<task>"` skeleton
- skill, mode, signal, constraint로 deterministic request routing
- source pointer가 있는 bounded repository context packing
- runtime-owned action candidate와 next gate reporting
- structured action line 또는 인식 가능한 action text에서 실행 없는 model action parsing
- local SQLite observability projection에 model/token/latency metric 기록
- intent, context pack, action candidate, model action, backend chat, model run ledger event
- versioned backend/model user agent를 쓰도록 source policy 정리
- 새 `run` 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

최신 Qwen3.5 smoke는 `model action parse: heuristic-text`, `model action kind:
patch-proposal`, `model action executable now: no`, `guard: pass`, `finish
reason: stop`을 반환했습니다. 이는 현재의 실행 없는 runtime boundary와
observability path의 증거이지, patch 품질이나 autonomous tool use 통과 증거는
아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 계속 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 patch 적용, command 실행, model output의 승인된 action 처리를 하지 않습니다.
- model action parsing은 tolerant하고 실행하지 않습니다. 안정적인 structured action 생성과 approval UI는 후속 작업입니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직 설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.1.0 - 개발자 프리뷰

릴리즈 날짜: 2026-07-06

이 버전은 `rolling-potato`의 첫 개발자 프리뷰입니다. 초기 Rust runtime과
CLI scaffold를 위한 source-only release tag이며, stable runtime contract가
아닙니다. 모델 가중치, 외부 plugin package, prebuilt model/backend bundle은
포함하지 않습니다.

### 포함된 것

- `rpotato` Rust CLI scaffold
- project/app state 초기화
- SQLite projection 기반 session list/new/resume
- runtime ledger와 evidence validation surface
- command/path policy check와 credential redaction
- hook registry와 fail-closed hook result validation
- local plugin import/inspect/validate/enable/disable/remove surface
- monitoring status, model summary, export, dry-run prune surface
- source-backed Qwen/Gemma model candidate manifest와 evaluation gate
- size와 SHA-256 검증이 있는 evaluation-only model artifact fetch
- managed `llama.cpp b9878` backend install/start/status/stop/health surface
- `/v1/chat/completions` 기반 non-streaming backend chat smoke path
- `chat_template_kwargs.enable_thinking=false`를 쓰는 Qwen3.5 non-thinking smoke path
- 기본 영문 문서와 주요 문서의 한국어 번역

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

Qwen chat smoke는 managed `llama.cpp` sidecar를 통해 깨끗한 한국어 응답을
반환했습니다. 이는 backend/model 연결과 non-thinking chat path의 증거이지,
전체 모델 품질 통과 증거는 아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 intent normalization만 수행하며 full agent loop는
  구현되지 않았습니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직
  설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.
