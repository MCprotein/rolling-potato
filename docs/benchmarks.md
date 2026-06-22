# Benchmarks

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

## runtime metric

수집할 metric:

- backend startup time
- first token latency
- tokens per second
- peak memory
- context tokens used
- regeneration count
- guard rejection count

## 후보 비교

초기 비교:

- `Qwen3.5-4B` quantized GGUF, 출처 확인 전 미확정
- `Gemma 4 E4B`, 출처 확인 전 미확정
- 참고용 `Qwen3.5-9B`

모든 후보는 같은 prompt compiler, 같은 context budget, 같은 tool policy로 평가합니다.

후보 모델의 license, artifact URL, checksum, backend 호환성은 [model-source-policy.md](model-source-policy.md)에 따라 확인된 뒤 benchmark 대상에 포함합니다.

## benchmark lane

두 종류의 benchmark를 분리합니다.

### Product benchmark

`rolling-potato`가 실제로 줄여야 하는 실패 모드를 측정합니다.

- 한국어 최종 응답 안정성
- 저장소 탐색과 온톨로지/source pointer 사용
- 작은 patch 생성과 diff 적용 가능성
- 검증 로그 해석
- command policy 준수
- runtime latency와 memory

### External benchmark parity

검색이나 model card에서 보이는 공개 benchmark 점수를 그대로 믿지 않고, 조건을 맞춰 재현 가능한지 확인합니다.

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
