# 모델 Knowledge Base

모델 knowledge base는 LLM에 대한 runtime evidence index입니다. 제품 논의에서는 LLM wiki라고 부를 수 있습니다.

이 문서는 model manifest, benchmark report, observability store, ontology graph를 대체하지 않습니다. 이 자료들을 연결해서 runtime이 어떤 모델이 후보인지, 왜 차단됐는지, 실제 실행에서 무엇이 관측됐는지 설명할 수 있게 합니다.

## 목적

- 모델 관련 claim을 source와 status와 함께 추적한다.
- 공개 benchmark claim과 local benchmark result를 연결한다.
- 모델, backend, quantization, task class, ontology view, prompt/runtime version별 반복 실패를 요약한다.
- 모델 능력을 상상하지 않고도 routing이 안전한 lane을 고르는 데 도움을 준다.
- TUI, `doctor`, report에 설명 가능한 모델 evidence를 제공한다.

## 기존 저장소와의 관계

- Model manifest는 install trust를 소유한다: artifact URL, provider terms, license, SHA-256, file size, backend compatibility, RAM evidence.
- Benchmark report는 측정된 product score와 public benchmark parity를 소유한다.
- Observability는 실제 run metric을 소유한다: token usage, latency, memory, guard, tool result, stop-gate result, failure category.
- Ontology는 source-backed semantic claim과 invariant check를 소유한다.
- Model knowledge base는 위 record를 index하고 요약한다. 별도 source of truth가 아니다.

## Claim 상태

Model knowledge entry는 명시적인 상태를 가져야 합니다.

- `observed`: runtime metric 또는 log에서 포착됐지만 아직 product claim은 아님
- `candidate`: 반복 evidence 때문에 조사할 가치가 있음
- `source-listed-unreproduced`: source에는 있지만 local에서 재현하지 않음
- `measured-locally`: 이 repository에서 조건을 기록하고 측정함
- `not-comparable`: source 조건과 local 조건이 너무 달라 직접 비교 불가
- `rejected`: 확인 결과 해당 claim에 쓸 수 없음
- `superseded`: 새 evidence로 대체됨

Knowledge base는 model manifest나 model source policy의 `confirmed` source record를 참조할 수 있습니다. 하지만 license/artifact claim을 혼자 `confirmed`로 만들면 안 됩니다.

## Claim Subject Taxonomy

각 entry는 어떤 종류의 claim인지 선언해야 합니다. subject가 어떤 store에 권한이 있고 어떤 promotion rule을 적용할지 결정합니다.

- `artifact_claim`
- `license_claim`
- `public_benchmark_claim`
- `local_benchmark_result`
- `runtime_observation`
- `routing_note`
- `ontology_view_observation`

상태 namespace는 분리합니다. Ontology claim state, manifest verification state, benchmark result state, model knowledge state는 서로 자동 변환되지 않습니다. 예를 들어 `measured-locally`는 `confirmed`와 같지 않고, `observed`는 license나 default model claim이 참이라는 evidence가 아닙니다.

## Observation과 Evidence

Observation은 run, log, metric, benchmark, guard result에서 포착된 사건입니다. Evidence는 runtime이나 maintainer가 그 observation을 다시 확인할 수 있게 하는 pointer입니다. 예: run id, benchmark id, artifact hash, source URL, source ref, redacted evidence artifact.

Observation count는 evidence quality를 대체하지 않습니다. 반복 observation은 우선순위를 높이거나 candidate note를 만들 수 있지만, 해당 claim의 authority store와 evidence requirement를 만족하기 전까지 claim을 증명하지 못합니다.

## 자동 관리

Agent는 다음 gate 안에서 model knowledge base를 자동으로 갱신할 수 있습니다.

1. ledger, benchmark, observability record에서 observation을 포착한다.
2. model id, artifact hash, backend, quantization, task class, ontology view, prompt/runtime version 기준으로 deduplicate한다.
3. 반복 pattern의 frequency counter를 올린다.
4. threshold를 넘으면 `observed` 또는 `candidate` note를 만든다.
5. benchmark run id, artifact hash, environment, prompt/runtime version, scoring result가 있을 때만 `measured-locally`로 승격한다.
6. source/license/artifact claim은 manifest/source-policy evidence를 통해서만 승격한다.
7. artifact, backend, quantization, prompt/runtime version, scoring method가 달라진 최신 evidence가 생기면 이전 entry를 `superseded`로 표시한다.

빈도는 우선순위를 높일 수 있습니다. 그러나 correctness, license, backend compatibility, RAM fit, Korean quality, default-model status를 확정할 수는 없습니다.

Frequency는 다음 field가 없으면 routing 근거로 사용할 수 없습니다.

- `sampleCount`
- `failureCount` 또는 `successCount`
- `timeWindow`
- `conditionKey`
- counter가 reset된 경우 `resetReason`
- 정확한 artifact/backend/quantization/prompt-runtime 조건

manifest, backend version, prompt compiler version, tool policy version, ontology view, artifact hash, scoring method, benchmark fixture checksum이 바뀌면 해당 frequency record를 reset 또는 supersede합니다.

## 빈도 Signal

빈도 기반 자동화는 triage에는 유용하지만 truth가 아닙니다.

유용한 signal:

- 같은 모델과 task class에서 반복되는 invalid diff
- 반복되는 source-read omission
- 반복되는 Korean guard failure
- 반복되는 tool-call parse failure
- 반복되는 stop-gate failure
- 같은 artifact/backend/quantization 조건에서 반복되는 성공
- 같은 ontology view에서 반복되는 context truncation
- 같은 small-model lane에서 반복되는 escalation

안전장치:

- candidate note 생성 전 minimum sample count를 요구한다.
- 서로 다른 artifact 조건을 합치지 않고 per-condition counter를 분리한다.
- manifest, prompt, backend, benchmark가 바뀌면 오래된 entry를 decay 또는 supersede한다.
- raw prompt와 raw source text는 기본적으로 knowledge base에 저장하지 않는다.
- run id, evidence id, benchmark id pointer를 저장한다.

## 권장 Record Shape

```json
{
  "id": "model-knowledge:qwen3.5-4b-q4-k-m:tool-use:2026-07",
  "modelId": "qwen3.5-4b-q4-k-m",
  "artifactSha256": "TODO",
  "backend": "llama.cpp",
  "quantization": "Q4_K_M",
  "taskClass": "tool-use",
  "ontologyView": "source-pointer-json-slice",
  "claimSubject": "runtime_observation",
  "claim": "Repeated tool-call parse failures observed in small patch fixtures.",
  "status": "observed",
  "frequency": 3,
  "sampleCount": 5,
  "failureCount": 3,
  "timeWindow": "2026-07",
  "conditionKey": "artifact+backend+quantization+promptRuntime+toolPolicy+ontologyView",
  "resetReason": null,
  "firstSeen": "2026-07-06T00:00:00Z",
  "lastSeen": "2026-07-06T00:00:00Z",
  "evidenceRefs": ["benchmark_run:TODO", "model_run:TODO"],
  "failureCategory": "tool execution or command interpretation failure",
  "responsibleSubsystem": "runtime-parser",
  "conditions": {
    "promptRuntimeVersion": "TODO",
    "toolPolicyVersion": "TODO",
    "promptCompilerVersion": "TODO",
    "contextLength": null,
    "sampling": "TODO"
  },
  "nextAction": "promote-to-regression-fixture"
}
```

이 shape는 예시입니다. `TODO` 값은 제품 사실이 아닙니다.

## Runtime 사용 위치

Runtime은 knowledge base를 다음 용도로 사용할 수 있습니다.

- 모델 후보가 왜 차단 또는 허용됐는지 표시
- benchmark 우선순위 선택
- 반복 실패가 있는 lane에서 작은 task를 다른 lane으로 routing
- 특정 model/task 조합에서 stop-gate failure가 반복되면 escalation 추천
- `doctor`와 TUI summary 생성

Model knowledge entry가 routing에 영향을 주면 runtime은 routing decision event를 기록해야 합니다.

- user request/session/workflow id
- 선택된 skill과 mode
- 선택된 model, backend, quantization, ontology view
- hint로 사용한 model knowledge entry id
- 참조한 policy, manifest, benchmark evidence
- escalation target 또는 fallback path
- 최종 decision reason

Runtime은 knowledge base를 다음 용도로 사용하면 안 됩니다.

- verified manifest entry 없이 모델 설치
- manifest, benchmark, runtime evidence 없이 default model 추천
- public leaderboard score를 local product result처럼 취급
- source-backed 또는 local measured evidence 없이 한국어/코드 성능이 더 좋다고 주장
- model source policy evidence 없이 license, checksum, artifact URL, RAM fit, backend compatibility 확정

## CLI와 TUI Surface

예정 surface:

- `rpotato model knowledge`
- `rpotato model knowledge inspect <model-id>`
- `rpotato model knowledge promote <entry-id> --dry-run`
- `rpotato model knowledge prune --before <duration> --dry-run`
- TUI model detail panel: manifest trust, benchmark status, runtime failure, knowledge note

모든 mutation command는 dry-run을 먼저 지원하고 ledger event를 기록해야 합니다.

## Safety Test

필수 negative test:

- frequency만으로 entry를 `measured-locally`로 승격할 수 없음
- model knowledge가 license, checksum, artifact URL, RAM fit, default-model claim을 만들 수 없음
- `promote --dry-run`과 `prune --dry-run`은 mutation을 기록하지 않음
- promotion/prune mutation은 ledger event를 기록함
- artifact/backend/quantization/prompt-runtime key가 다르면 merge하지 않음
- stale entry는 조용히 overwrite하지 않고 supersede함
- raw prompt와 raw source는 기본적으로 knowledge record에 저장하지 않음
