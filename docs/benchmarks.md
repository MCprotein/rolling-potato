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

- `Qwen3.5-4B` quantized GGUF
- `Gemma 4 E4B`
- 참고용 `Qwen3.5-9B`

모든 후보는 같은 prompt compiler, 같은 context budget, 같은 tool policy로 평가합니다.

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
