# Benchmarks

This document is the draft for turning [model-eval.md](model-eval.md) into an executable benchmark suite.

## Goals

Measure `rolling-potato` product failure modes, not general model scores.

Targets:

- final Korean response stability
- repository exploration accuracy
- small patch generation ability
- diff applicability
- verification log interpretation
- command policy compliance
- runtime latency and memory

## Benchmark Fixture Structure

Planned structure:

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

Fixtures should be small and independent. Each fixture measures one failure mode.

## Common Scoring

Each task is scored from 0 to 3.

- 0: failed, risky, or format collapse
- 1: partially useful but requires manual recovery
- 2: mostly successful, needs small verification
- 3: stable success

Minimum pass criteria:

- average score at least 2.2
- final Korean response failure rate at most 5%
- invalid diff rate at most 10%
- destructive action policy violations: 0

## Runtime Metrics

Metrics to collect:

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

## Candidate Comparison

Initial comparison:

- `Qwen3.5-4B` quantized GGUF, unconfirmed before artifact/runtime verification
- `Gemma 4 E4B`, unconfirmed before artifact/runtime verification
- reference-only `Qwen3.5-9B`

All candidates are evaluated with the same prompt compiler, context budget, and tool policy.

Candidate license, artifact URL, checksum, and backend compatibility are checked under [model-source-policy.md](model-source-policy.md) before inclusion in benchmarks.

## Benchmark Lanes

Two benchmark types are separated.

### Product Benchmark

Measures the failure modes `rolling-potato` must reduce:

- final Korean response stability
- repository exploration and ontology/source-pointer use
- small patch generation and diff applicability
- verification log interpretation
- command policy compliance
- runtime latency and memory

### External Benchmark Parity

Do not trust public benchmark scores found through search or model cards as-is. Reproduce comparable items under matched conditions where possible.

Current Phase 5 implementation records public benchmark source URLs and `source-listed-unreproduced` status in the manifest. This is not a confirmed product score; it means reproducibility evaluation is pending. Do not compare public scores directly until local score, hardware/backend condition, quantization, dataset, and scoring method are recorded.

Each benchmark item should include:

- published score source URL
- checked-at date
- benchmark harness and version or commit
- dataset name, version, and license
- prompt/template and scoring method
- whether the score is for upstream original model or quantized GGUF artifact
- backend, quantization, context length, sampling options
- local score
- difference between published and local score
- reason when conditions are too different for direct comparison

Public benchmark parity is supporting evidence only. MVP default-model decisions also consider product benchmark, 16 GB runtime fit, license/source/checksum verification, and Korean guard results.

## Publication Criteria

Published benchmark results must include:

- OS
- CPU/GPU
- RAM
- backend version
- model artifact URL
- SHA-256
- quantization
- prompt/runtime version

Publishing only results without artifact information is not reproducible and is not allowed.

## Observability Integration

Benchmark runs should use the same metric schema as normal runtime monitoring.

- `benchmark_runs` links to model/backend/session metrics.
- Published-vs-local score comparison stores artifact hash and runtime options.
- Raw prompt/source text is not stored long term during benchmarks.
- Benchmark reports are generated from SQLite projection, but JSONL export should be available for reproducibility.
