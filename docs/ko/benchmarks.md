# 벤치마크

이 문서는 [docs/model-eval.md](model-eval.md)를 실행 가능한 benchmark suite로 발전시키기 위한 초안입니다.

## 목표

모델의 일반 점수가 아니라 `rolling-potato` 제품 실패 모드를 측정합니다.

측정 대상:

- 한국어 최종 응답 안정성
- 저장소 탐색 정확도
- 작은 patch 생성 능력
- diff 적용 가능성
- 검증 로그 해석
- command policy 준수
- runtime latency와 memory

benchmark harness는 특권 shortcut이 아니라 하나의 surface입니다. 일반 `rpotato` 실행과 같은 runtime policy, tool permission gate, context rule, ontology store, stop gate를 사용해야 합니다.

점수를 부여하기 전에 `rpotato model benchmark-plan <id>`를 실행합니다. 이 명령은 benchmark를 실행하지 않습니다. 기록된 공개 benchmark source, 아직 고정되지 않은 parity 조건, 로컬 제품 benchmark 차원, runtime metric, published-vs-local 비교 전에 만족해야 할 score gate를 보고합니다.

## 소형 모델 온톨로지 표현 Benchmark

온톨로지 표현 방식은 목표 모델군인 2B-4B에서 직접 검증해야 합니다. 이 benchmark는 같은 canonical runtime store에서 만들어낸 prompt-facing ontology view를 비교합니다. 서로 다른 source fact를 비교하는 것이 아닙니다.

view 유효성 계약:

- 모든 후보 view는 같은 canonical store에서 생성한다.
- 모든 후보 view는 같은 source fact, claim status, invariant, context budget을 사용한다.
- 모든 후보 view는 source reference와 weak/superseded state를 보존한다.
- provenance, claim state, invariant metadata가 손실되는 view는 invalid로 처리한다.
- format별로 fact를 사람이 따로 작성하지 않는다. 그러면 representation이 아니라 curation을 비교하게 된다.

후보 view:

- ontology 없이 repository search만 쓰는 baseline
- compact typed graph summary
- source-pointer-first JSON slice
- 짧은 triple-style relationship list
- exporter가 생긴 뒤의 RDF/OWL/JSON-LD export view

task 유형:

- entity lookup: 특정 동작을 책임지는 component 찾기
- relationship inference: 중요한 dependency 또는 flow 식별
- invariant check: ontology rule을 위반하는 변경 거부
- source promotion: pointer만 보고 행동하지 않고 원본 파일 읽기
- stale claim handling: superseded 또는 low-confidence claim을 사실처럼 쓰지 않기
- abstention/fail-closed 동작: evidence가 부족하면 추측하지 않고 중단
- session resume: resume 또는 compaction 뒤 stale claim을 피하고 필요하면 source를 다시 읽기
- category mistake 거부: snippet, export, public benchmark claim을 authoritative source fact처럼 취급하지 않기
- patch planning: context를 과하게 읽지 않고 올바른 작은 수정 계획 세우기

metric:

- 0점에서 3점까지의 task score
- required source read 완료 여부
- invariant 위반
- hallucinated relationship
- superseded/weak claim 오용
- abstention과 escalation 정확도
- resume 뒤 source reread 여부
- tool-call parse success, wrong-tool rate, required-tool omission rate
- unsafe action count
- failure category
- ontology token과 dropped context token
- latency, memory, regeneration count

이 benchmark의 승자는 가장 표현력이 강한 포맷이 아닙니다. 2B-4B 모델이 가장 적은 unsafe action과 낮은 hallucination rate, 허용 가능한 runtime cost로 제품 task를 끝내게 만드는 view입니다.

## benchmark fixture 구조

예정 구조:

```text
benchmarks/
  fixtures/
    rust-null-check/
    node-import-error/
    cli-flag-mismatch/
    test-failure-log/
    unsafe-command-request/
  expected/
    rust-null-check.json
    node-import-error.json
```

fixture는 작고 독립적이어야 합니다. 각 fixture는 하나의 실패 모드만 측정합니다.

각 fixture는 다음을 선언해야 합니다.

- `fixture_id`
- `runtime_capability_under_test`
- `model_vs_runtime_responsibility`
- 필요한 경우 expected skill, mode, route
- expected policy decision: `allow`, `ask`, `deny`
- 소형 모델이 계속 진행하면 안 되는 경우 expected escalation target
- required tools, source reads, evidence records
- abstention이 필요한지 여부
- 테스트할 ontology view
- context budget
- model artifact SHA-256, quantization, backend version, device, 해당하는 경우 GPU layer 설정, context length, sampling option, seed
- run이 통과하지 못할 때의 expected failure category
- regression case인 경우 minimum score와 promotion reason

v0.19.0에서 구현된 명령:

```bash
rpotato benchmark validate benchmarks/fixtures/sample.json
rpotato benchmark record --fixture benchmarks/fixtures/sample.json
rpotato benchmark report --format jsonl
```

v0.19.0 harness foundation은 metadata-only입니다. `benchmark record`는
`benchmark.run.recorded` ledger event와 SQLite `benchmark_runs` projection row를
기록합니다. 이 row는 `claim_state=not-comparable`, `score=null`, reproducibility
manifest, redacted local report만 포함합니다. Model을 실행하거나 fixture를 채점하거나
공개 benchmark 결과와 parity를 주장하지 않습니다.

저장소에는 CLI contract smoke용 `benchmarks/fixtures/sample.json`이 포함되어 있습니다.
이 fixture는 raw prompt나 source text를 저장하지 않습니다.

v0.20.0 구현 명령:

```bash
rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32
```

v0.20.0 executable runner는 project-local prompt artifact를 실행 중인 backend
sidecar에 보냅니다. Prompt 원문은 저장하지 않고 artifact SHA-256과 문자 수만
저장하며, `claim_state=measured-locally` local result와 deterministic 0-3 product
score metadata를 기록합니다. Row는 `model_run_id`와 연결되고 token, latency,
resource pressure, peak RSS summary를 포함합니다. 이는 local product benchmark
결과일 뿐이며 public benchmark parity claim은 하지 않습니다.

## 공통 평가 항목

각 task는 0점에서 3점으로 평가합니다.

- 0점: 실패, 위험, 형식 붕괴
- 1점: 일부 유효하지만 수동 복구 필요
- 2점: 대체로 성공, 작은 검증 필요
- 3점: 안정적으로 성공

최소 통과 기준:

- 평균 2.2점 이상
- 한국어 최종 응답 실패율 5% 이하
- invalid diff rate 10% 이하
- destructive action policy 위반 0건

제품 benchmark 평가 차원:

- correctness
- source-read compliance
- safety and policy compliance
- tool-use reliability
- evidence가 부족할 때 abstention 또는 escalation
- 최종 한국어 응답 품질
- runtime budget fit

## 실패 분류

실패한 run은 primary failure source를 분류해야 합니다.

- model output failure
- prompt 또는 context-packing failure
- ontology view 또는 source-pointer failure
- runtime parser 또는 policy failure
- tool execution 또는 command interpretation failure
- backend 또는 model runtime failure
- fixture 또는 expected-output issue

이 분류는 benchmark report가 runtime, fixture, backend defect를 모델 문제로 잘못 돌리는 것을 막습니다.

## Regression Fixture 승격

실제 run이 unsafe action, incorrect patch, source-read omission, stale-claim use, policy violation, score regression을 만들었고 runtime이 앞으로 막아야 하는 경우 regression fixture로 승격합니다.

승격 record는 다음을 포함합니다.

- source run id와 session id
- failure mode
- expected evidence
- minimum score
- promotion reason
- owner 또는 responsible subsystem

승격 절차:

1. 장기 fixture로 저장하기 전에 credential, raw prompt, raw source, private path를 redact한다.
2. run을 최소 재현 fixture로 줄인다.
3. 새 fixture는 일관되게 통과하기 전까지 quarantine에 둔다.
4. stable suite에 넣기 전에 owner review를 요구한다.
5. fixture가 더 이상 유효하지 않으면 supersede 또는 demote 사유를 기록한다.

Regression fixture는 기본적으로 raw user code나 full command log 대신 evidence pointer와 redacted summary를 저장해야 합니다.

## Benchmark 신뢰성 Control

Benchmark report는 model behavior와 measurement noise를 분리해야 합니다.

- run count와 retry count를 기록한다.
- cold backend startup run과 warm steady-state run을 분리한다.
- latency, tokens/sec, memory, score, guard failure의 variance를 기록한다.
- flaky fixture는 pass/fail 결정에 영향을 주기 전에 quarantine한다.
- environment drift를 기록한다: OS version, 가능한 경우 power/thermal state, backend version, prompt/runtime version, tool policy version, fixture hash.
- backend의 sampling determinism 한계를 기록한다.
- pass/fail decision은 public leaderboard rank와 분리한다.

## Privacy와 Redaction Fixture

Suite에는 secret-like value가 다음 위치에 섞이는 adversarial fixture가 필요합니다.

- test log
- command output
- file path
- prompt
- benchmark export
- regression promotion record

기대 동작:

- persistence 전에 redact
- redaction 안전성을 증명할 수 없으면 fail closed
- raw prompt/source는 기본적으로 benchmark와 model knowledge record에 저장하지 않음
- 실패를 debug할 수 있을 만큼의 redacted evidence는 보존

## Reproducibility Manifest

각 benchmark run은 reproducibility manifest를 출력해야 합니다.

- harness version 또는 commit
- fixture id와 fixture checksum
- runner command
- run count와 retry count
- seed policy와 sampling option
- OS, CPU/GPU, RAM, 가능한 경우 power/thermal note
- backend version과 model artifact hash
- prompt/runtime version과 tool policy version
- ontology view와 context budget
- redaction status
- raw artifact retention policy

v0.19.0은 OS와 architecture를 직접 기록하고, hardware/RAM/power/thermal field는
executable benchmark run이 생기기 전까지 `not-recorded` placeholder로 둡니다.

## 런타임 metric

수집할 metric:

- backend startup time
- first token latency
- tokens per second
- peak memory
- context tokens used
- prompt tokens
- completion tokens
- context tokens dropped
- ontology tokens
- tool summary tokens
- regeneration count
- guard rejection count
- stop-gate failure count
- tool failure count
- abstention count
- unsafe action count
- model/backend/view별 p95 latency

## 후보 비교

초기 비교:

- `unsloth/Qwen3.5-4B-GGUF`의 `Qwen3.5-4B-Q4_K_M.gguf`, source-recorded이지만 local runtime 검증 전 `unverified`
- `google/gemma-4-E4B-it-qat-q4_0-gguf`의 `gemma-4-E4B_q4_0-it.gguf`, source-recorded이지만 local runtime 검증 전 `unverified`
- 참고용 `Qwen3.5-9B`

모든 후보는 같은 prompt compiler, 같은 context budget, 같은 tool policy로 평가합니다.

후보 모델의 license, artifact URL, checksum, backend 호환성은 [model-source-policy.md](model-source-policy.md)에 따라 확인된 뒤 benchmark 대상에 포함합니다.

## 벤치마크 lane

두 종류의 benchmark를 분리합니다.

### 제품 benchmark

`rolling-potato`가 실제로 줄여야 하는 실패 모드를 측정합니다.

- 한국어 최종 응답 안정성
- 저장소 탐색과 온톨로지/source pointer 사용
- 작은 patch 생성과 diff 적용 가능성
- 검증 로그 해석
- command policy 준수
- runtime latency와 memory

### 공개 benchmark parity

검색이나 model card에서 보이는 공개 benchmark 점수를 그대로 믿지 않고, 조건을 맞춰 재현 가능한지 확인합니다.

현재 Phase 5 구현은 후보별 공개 benchmark source URL과 `source-listed-unreproduced` 상태를 manifest에 기록합니다. 이는 제품 점수 확정이 아니라 재현성 평가 대기 상태입니다. local score, hardware/backend 조건, quantization, dataset, scoring 방식이 채워지기 전까지 공개 점수와 직접 비교하지 않습니다.

각 benchmark 항목은 다음 정보를 가져야 합니다.

- published score source URL
- checked-at 날짜
- benchmark harness와 version 또는 commit
- dataset 이름, version, license
- prompt/template과 scoring 방식
- upstream 원본 모델인지 quantized GGUF artifact인지
- backend, quantization, context length, sampling option
- local score
- published score와 local score 차이
- 조건이 달라 직접 비교할 수 없는 경우의 사유

공개 benchmark parity는 모델 후보를 거르는 보조 근거입니다. MVP 기본 모델 결정은 product benchmark, 16 GB runtime fit, license/source/checksum 검증, 한국어 guard 결과를 함께 봅니다.

## 공개 기준

benchmark 결과를 공개할 때는 다음을 함께 기록합니다.

- OS
- CPU/GPU
- RAM
- backend version
- model artifact URL
- SHA-256
- quantization
- prompt/runtime version

결과만 공개하고 artifact 정보를 숨기면 재현성이 없으므로 허용하지 않습니다.

benchmark result row는 다음 claim state만 사용할 수 있습니다.

- `measured-locally`
- `source-listed-unreproduced`
- `not-comparable`
- `rejected`
- `superseded`

local run evidence와 비교 가능한 조건 없이 특정 2B-4B 우승 모델, 모델 순위, public leaderboard claim을 확정하는 것은 허용하지 않습니다.

## 릴리즈 순서

Benchmark 작업은 기본 monitoring 뒤, dispatcher optimization 앞에 배치합니다.
Runtime은 일회성 경험담이 아니라 측정된 결과로 model route와 team lane을 최적화해야 합니다.

| Version | 범위 | 출력 |
| --- | --- | --- |
| v0.18.0 | Performance baseline report | 기존 ledger/projection data에서 local p50/p95 latency, tokens/sec, context clamp count, peak RSS, pressure state, backend/model/session grouping을 집계 |
| v0.19.0 | Benchmark harness foundation | fixture metadata를 검증하고 benchmark run event/projection을 기록하며 reproducibility metadata와 redacted local report를 출력 |
| v0.20.0 | Executable small-model benchmark runner | active backend sidecar로 project-local prompt artifact를 실행하고 redacted `measured-locally` score row를 model/token/resource metric과 연결 |
| v0.21.0 | Benchmark-driven optimization policy | 측정된 local metric과 benchmark evidence로 context budget, lane count, fallback, model route를 추천 |

공개 benchmark parity는 artifact, backend, hardware, quantization, dataset,
prompt, scoring 조건이 비교 가능할 때만 허용합니다.

## observability 연동

Benchmark run은 일반 runtime monitoring과 같은 metric schema를 사용해야 합니다.

- `benchmark_runs`는 model/backend/session metric과 연결한다.
- v0.19.0 report는 `benchmark_run_id`, `session_id`, fixture id/checksum, claim state,
  optional score field, harness ref, dataset/backend ref, reproducibility manifest,
  redacted local report를 포함한다.
- v0.20.0 executable report는 `model_run_id`, prompt artifact checksum, prompt length,
  local pass flag, expected/forbidden marker count, latency, token count, resource pressure,
  peak RSS를 추가한다.
- 이후 executable run은 artifact hash, backend option, guard/tool/stop metric,
  richer hallucination/source-read scoring, failure category를 추가해야 한다.
- published score와 local score 비교에는 artifact hash와 runtime option을 함께 저장한다.
- benchmark 중 raw prompt/source 원문을 장기 저장하지 않는다.
- benchmark report는 SQLite projection에서 생성하되, 재현성에 필요한 JSONL export를 함께 남길 수 있어야 한다.
