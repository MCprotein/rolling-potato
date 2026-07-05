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

The benchmark harness is a surface, not a privileged shortcut. It must use the
same runtime policy, tool permission gates, context rules, ontology store, and
stop gate as normal `rpotato` runs.

## Small-Model Ontology Representation Benchmark

The ontology format decision must be tested against the target 2B-4B model class.
The benchmark compares prompt-facing ontology views produced from the same
canonical runtime store. It does not compare different source facts.

View validity contract:

- every candidate view is generated from the same canonical store
- every candidate view uses the same source facts, claim statuses, invariants,
  and context budget
- every candidate view preserves source references and weak/superseded states
- a view that loses provenance, claim state, or invariant metadata is invalid
- hand-authored facts per format are not allowed because they compare curation,
  not representation

Candidate views:

- no-ontology baseline with repository search only
- compact typed graph summary
- source-pointer-first JSON slice
- short triple-style relationship list
- RDF/OWL/JSON-LD export view, only after an exporter exists

Task types:

- entity lookup: find the component responsible for a behavior
- relationship inference: identify the dependency or flow that matters
- invariant check: reject a change that violates an ontology rule
- source promotion: read the original file before acting on a pointer
- stale claim handling: avoid using superseded or low-confidence claims as facts
- abstention and fail-closed behavior: stop instead of guessing when evidence is
  missing
- session resume: avoid stale claims after resume or compaction and reread
  source when needed
- category-mistake rejection: reject treating snippets, exports, or public
  benchmark claims as authoritative source facts
- patch planning: propose the right small edit without over-reading context

Metrics:

- task score from 0 to 3
- required source reads completed
- invariant violations
- hallucinated relationships
- superseded/weak claim misuse
- abstention and escalation correctness
- source reread after resume
- tool-call parse success, wrong-tool rate, and required-tool omission rate
- unsafe action count
- failure category
- ontology tokens and dropped context tokens
- latency, memory, and regeneration count

The winning view is not the most expressive format. It is the view that makes
2B-4B models complete product tasks with the fewest unsafe actions, lowest
hallucination rate, and acceptable runtime cost.

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

Each fixture should declare:

- `fixture_id`
- `runtime_capability_under_test`
- `model_vs_runtime_responsibility`
- expected skill, mode, and route when applicable
- expected policy decision: `allow`, `ask`, or `deny`
- expected escalation target when the small model should not continue
- required tools, source reads, and evidence records
- whether abstention is required
- ontology view under test
- context budget
- model artifact SHA-256, quantization, backend version, device, GPU layer
  setting when applicable, context length, sampling options, and seed
- expected failure category when the run does not pass
- minimum score and promotion reason when the fixture is a regression case

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

Product benchmark scoring dimensions:

- correctness
- source-read compliance
- safety and policy compliance
- tool-use reliability
- abstention or escalation when evidence is insufficient
- final Korean response quality
- runtime budget fit

## Failure Taxonomy

Failed runs must classify the primary failure source:

- model output failure
- prompt or context-packing failure
- ontology view or source-pointer failure
- runtime parser or policy failure
- tool execution or command interpretation failure
- backend or model runtime failure
- fixture or expected-output issue

The taxonomy prevents benchmark reports from blaming the model for runtime,
fixture, or backend defects.

## Regression Fixture Promotion

A real run should become a regression fixture when it produced an unsafe action,
incorrect patch, source-read omission, stale-claim use, policy violation, or
score regression that the runtime should prevent in the future.

Promotion records include:

- source run id and session id
- failure mode
- expected evidence
- minimum score
- promotion reason
- owner or responsible subsystem

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
- stop-gate failure count
- tool failure count
- abstention count
- unsafe action count
- p95 latency by model/backend/view

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

Benchmark result rows may use only these claim states:

- `measured-locally`
- `source-listed-unreproduced`
- `not-comparable`
- `rejected`
- `superseded`

Speculative 2B-4B winners, model rankings, or public leaderboard claims are not
allowed without local run evidence and comparable conditions.

## Observability Integration

Benchmark runs should use the same metric schema as normal runtime monitoring.

- `benchmark_runs` links to model/backend/session metrics.
- reports include `run_id`, `session_id`, `model_run_id`, artifact hash, backend
  options, guard/tool/stop metrics, and failure category
- Published-vs-local score comparison stores artifact hash and runtime options.
- Raw prompt/source text is not stored long term during benchmarks.
- Benchmark reports are generated from SQLite projection, but JSONL export should be available for reproducibility.
