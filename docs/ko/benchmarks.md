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

## 소형 모델 온톨로지 표현 Benchmark

온톨로지 표현 방식은 목표 모델군인 2B-4B에서 직접 검증해야 합니다. 이 benchmark는 같은 canonical runtime store에서 만들어낸 prompt-facing ontology view를 비교합니다. 서로 다른 source fact를 비교하는 것이 아닙니다.

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
- patch planning: context를 과하게 읽지 않고 올바른 작은 수정 계획 세우기

metric:

- 0점에서 3점까지의 task score
- required source read 완료 여부
- invariant 위반
- hallucinated relationship
- superseded/weak claim 오용
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

## 후보 비교

초기 비교:

- `Qwen3.5-4B` quantized GGUF, artifact/runtime 검증 전 미확정
- `Gemma 4 E4B`, artifact/runtime 검증 전 미확정
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

## observability 연동

Benchmark run은 일반 runtime monitoring과 같은 metric schema를 사용해야 합니다.

- `benchmark_runs`는 model/backend/session metric과 연결한다.
- published score와 local score 비교에는 artifact hash와 runtime option을 함께 저장한다.
- benchmark 중 raw prompt/source 원문을 장기 저장하지 않는다.
- benchmark report는 SQLite projection에서 생성하되, 재현성에 필요한 JSONL export를 함께 남길 수 있어야 한다.
