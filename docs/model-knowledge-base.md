# Model Knowledge Base

The model knowledge base is the runtime's evidence index for LLMs. It is also
called the LLM wiki in product discussions.

It does not replace the model manifest, benchmark reports, observability store,
or ontology graph. It connects those sources so the runtime can explain why a
model is a candidate, why it is blocked, and what real runs have shown.

## Purpose

- Track model-related claims with source and status.
- Connect public benchmark claims to local benchmark results.
- Summarize repeated runtime failures by model, backend, quantization, task
  class, ontology view, and prompt/runtime version.
- Help routing choose safe default lanes without inventing model capability.
- Feed TUI, `doctor`, and reports with explainable model evidence.

## Relationship To Existing Stores

- Model manifest owns install trust: artifact URL, provider terms, license,
  SHA-256, file size, backend compatibility, and RAM evidence.
- Benchmark reports own measured product scores and public benchmark parity.
- Observability owns real run metrics: token usage, latency, memory, guards,
  tool results, stop-gate results, and failure categories.
- Ontology owns source-backed semantic claims and invariant checks.
- The model knowledge base indexes and summarizes those records. It is not a
  separate source of truth.

## Claim States

Model knowledge entries must use explicit states.

- `observed`: captured from runtime metrics or logs, not a product claim yet
- `candidate`: worth investigating because repeated evidence points to it
- `source-listed-unreproduced`: listed by a source, not reproduced locally
- `measured-locally`: measured by this repository under recorded conditions
- `not-comparable`: source and local conditions differ too much
- `rejected`: checked and not usable for the stated claim
- `superseded`: replaced by newer evidence

The knowledge base can reference `confirmed` source records from the model
manifest or model source policy, but it should not create a confirmed
license/artifact claim by itself.

## Claim Subject Taxonomy

Each entry must declare what kind of claim it is about. The subject controls
which store has authority and which promotion rules apply.

- `artifact_claim`
- `license_claim`
- `public_benchmark_claim`
- `local_benchmark_result`
- `runtime_observation`
- `routing_note`
- `ontology_view_observation`

State namespaces are separate. Ontology claim states, manifest verification
states, benchmark result states, and model knowledge states are not converted
into each other automatically. For example, `measured-locally` is not the same
thing as `confirmed`, and `observed` is not evidence that a license or default
model claim is true.

## Observation And Evidence

An observation is an event captured from a run, log, metric, benchmark, or
guard result. Evidence is the pointer that lets the runtime or maintainer
re-check that observation, such as a run id, benchmark id, artifact hash,
source URL, source ref, or redacted evidence artifact.

Observation count does not replace evidence quality. A repeated observation can
raise priority or create a candidate note, but it cannot prove the claim unless
the claim's authority store and evidence requirements are satisfied.

## Automatic Management

Agents may update the model knowledge base automatically, but only inside these
gates:

1. Capture observations from ledger, benchmark, and observability records.
2. Deduplicate by model id, artifact hash, backend, quantization, task class,
   ontology view, and prompt/runtime version.
3. Increase frequency counters for repeated patterns.
4. Create `observed` or `candidate` notes when thresholds are crossed.
5. Promote to `measured-locally` only when a benchmark run id, artifact hash,
   environment, prompt/runtime version, and scoring result are present.
6. Promote source/license/artifact claims only through manifest/source-policy
   evidence.
7. Mark older entries `superseded` when newer evidence uses a different
   artifact, backend, quantization, prompt/runtime version, or scoring method.

Frequency alone can raise priority. It cannot confirm correctness, license,
backend compatibility, RAM fit, Korean quality, or default-model status.

Frequency cannot influence routing unless the record also includes:

- `sampleCount`
- `failureCount` or `successCount`
- `timeWindow`
- `conditionKey`
- `resetReason` when counters were reset
- exact artifact/backend/quantization/prompt-runtime conditions

Changing manifest, backend version, prompt compiler version, tool policy
version, ontology view, artifact hash, scoring method, or benchmark fixture
checksum resets or supersedes the affected frequency record.

## Frequency Signals

Frequency-based automation is useful for triage, not truth.

Useful signals:

- repeated invalid diffs for the same model and task class
- repeated source-read omissions
- repeated Korean guard failures
- repeated tool-call parse failures
- repeated stop-gate failures
- repeated success under the same artifact/backend/quantization conditions
- repeated context truncation for the same ontology view
- repeated escalation from the same small-model lane

Safeguards:

- require a minimum sample count before creating candidate notes
- keep per-condition counters separate instead of merging different artifacts
- decay or supersede stale entries after manifest, prompt, backend, or benchmark
  changes
- keep raw prompt and raw source text out of the knowledge base by default
- store pointers to run ids, evidence ids, and benchmark ids instead

## Suggested Record Shape

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

This shape is illustrative. `TODO` values are not product facts.

## Runtime Use

The runtime may use the knowledge base to:

- show why a model candidate is blocked or allowed
- choose benchmark priorities
- route small tasks away from lanes with repeated failures
- recommend escalation when a model/task combination has repeated stop-gate
  failures
- generate `doctor` and TUI summaries

When a model knowledge entry influences routing, the runtime should record a
routing decision event with:

- user request/session/workflow ids
- selected skill and mode
- selected model, backend, quantization, and ontology view
- model knowledge entry ids used as hints
- policy, manifest, and benchmark evidence consulted
- escalation target or fallback path
- final decision reason

The runtime must not use the knowledge base to:

- install a model without a verified manifest entry
- recommend a default model without manifest, benchmark, and runtime evidence
- treat public leaderboard scores as local product results
- claim a model is better for Korean/code without source-backed or local
  measured evidence
- confirm license, checksum, artifact URL, RAM fit, or backend compatibility
  without the model source policy evidence

## CLI And TUI Surface

Planned surfaces:

- `rpotato model knowledge`
- `rpotato model knowledge inspect <model-id>`
- `rpotato model knowledge promote <entry-id> --dry-run`
- `rpotato model knowledge prune --before <duration> --dry-run`
- TUI model detail panel: manifest trust, benchmark status, runtime failures,
  and knowledge notes

All mutation commands should support dry-run first and record ledger events.

## Safety Tests

Required negative tests:

- frequency alone cannot promote an entry to `measured-locally`
- model knowledge cannot create license, checksum, artifact URL, RAM fit, or
  default-model claims
- `promote --dry-run` and `prune --dry-run` record no mutation
- promotion/prune mutations record ledger events
- mismatched artifact/backend/quantization/prompt-runtime keys do not merge
- stale entries are superseded instead of silently overwritten
- raw prompt and raw source are not persisted in knowledge records by default
