# Ontology Runtime

Ontology is the core runtime memory of `rolling-potato`.

Small models cannot reliably reconstruct project structure, terminology, ownership, workflows, and invariants from prompts every turn. The runtime core manages this meaning structure as ontology so it can reduce model freedom and prevent repeated mistakes.

## Canonical Store

The canonical ontology is a runtime data model, not a human-edited YAML file.

`anamnesis` uses YAML because it is a library/config lifecycle tool and humans must review, edit, and commit its project memory. `rolling-potato` is an agent runtime. The runtime should persist ontology as typed graph records with provenance and query indexes, then render that graph into TUI, HTML, JSON, YAML, or other views only when humans need to inspect it.

Canonical means the data model and storage layer the runtime trusts when it plans, retrieves context, checks invariants, resumes sessions, and compiles prompts. For `rolling-potato`, the target canonical shape is:

- typed graph records for entities, relationships, flows, invariants, ownership, decisions, and open questions
- source references and evidence records attached to every semantic claim
- append-only ledger events for audit and replay
- SQLite-backed indexes/projections for fast local queries, TUI views, diagnostics, and benchmark reports
- import/export serializers for JSON/YAML fixtures and future interchange formats

YAML and JSON are serialization formats, not the ontology itself. They can seed fixtures, export debug snapshots, or support migration, but they must not become the source of truth for runtime reasoning.

RDF, OWL, JSON-LD, Turtle, or SHACL are also not the default canonical store yet. They remain possible import/export or interoperability targets if the project later needs semantic-web tooling, external reasoners, or standard ontology exchange. Until that requirement is proven, the runtime prioritizes a compact typed graph optimized for local agent-loop latency, source-backed claims, and predictable queries.

## Small-Model Fit Gate

The ontology representation must be chosen by measuring 2B-4B model behavior, not by assuming that a formally richer format helps small models.

Before locking the prompt-facing ontology view, benchmark at least these representations against the same canonical store:

- compact typed graph summary
- source-pointer-first JSON slice
- short triple-style relationship list
- RDF/OWL/JSON-LD export view if an exporter exists
- no-ontology baseline with repository search only

Each candidate view is evaluated on whether small models can:

- identify the correct entity and relationship for a task
- obey invariants and ownership boundaries
- promote source pointers to original-file reads before action
- avoid treating weak or superseded claims as confirmed facts
- preserve final Korean response quality
- stay within token, latency, and memory budgets

If a formal export format improves model behavior under the same budget, it can become a supported view. If it only improves interchange with external tools, it remains an import/export surface and not the runtime canonical store.

## Goals

- Maintain project meaning structure as a runtime asset.
- Reduce information that small models must recover through guesswork.
- Narrow context packing around source pointers.
- Check whether agent actions violate project invariants.
- Preserve work intent and decision evidence across sessions.

## Two Layers

### Layer A: Deterministic Facts

Facts the runtime can collect directly.

Examples:

- file path
- file hash
- package manager
- test script
- build script
- public symbol
- entrypoint candidate
- generated/vendor exclusion

Layer A records freshness and source hash instead of confidence.

### Layer B: Semantic Ontology

Project meaning structure.

Examples:

- domain entity
- relationship
- ownership
- invariant
- workflow
- architecture boundary
- open question
- rejected decision

Layer B entries must have source references, confidence, and status.

Status examples:

- proposed
- confirmed
- superseded
- rejected
- open_question

## Core Graph Shape

```json
{
  "schemaVersion": 1,
  "entities": [],
  "relationships": [],
  "flows": [],
  "invariants": [],
  "ownership": [],
  "openQuestions": [],
  "sourceRefs": []
}
```

This JSON shape is documentation for the runtime graph contract, not the canonical storage format. The implementation can normalize it into tables or graph indexes as long as it preserves stable IDs, provenance, and replayability.

Each semantic assertion must include:

- `id`
- `kind`
- `statement`
- `sourceRefs`
- `confidence`
- `status`
- `updatedAt`
- `supersedes` when it replaces a prior assertion

Runtime records should also preserve:

- deterministic source hash or observation hash for Layer A facts
- provenance: generator, model/backend, command, session, and ledger event when applicable
- scope path for monorepos or nested project roots
- conflict and drift state when a newer source observation contradicts a prior claim

## `anamnesis` Concepts To Absorb

`rolling-potato` should absorb the lifecycle design, not the YAML-as-canonical storage model.

- Layer A: deterministic local introspection creates verifiable facts.
- Layer B: semantic enrichment creates relationships, flows, operational rules, and open questions only with source evidence.
- Regenerable facts and reviewed semantics must be separated so a refresh cannot overwrite curated meaning.
- Re-runs merge by stable IDs, append new claims, mark replacements with `supersedes`, and put weak evidence under open questions.
- Context indexes and resume bundles should carry source pointers and snippets, not replace authoritative source reads.
- Diagnostics should report missing facts, stale observations, source-less claims, duplicate IDs, contradictory relationships, and superseded entries treated as current.

The runtime version of this design stores those concepts in the ontology graph and ledger, then renders human-readable views on demand.

## Runtime Use

### `rpotato init`

- create project identity
- create ontology store/schema and seed Layer A facts
- prepare project-local `.rpotato/` state layout
- display ontology gap diagnostics

### `rpotato run`

- find ontology entries related to the user request
- promote source pointers to original-file reads
- pass only the minimum required entries to the prompt compiler
- check whether action candidates violate invariants
- record apply/verification evidence in the ledger
- update ontology observations only through source-backed events

### `rpotato doctor`

- check Layer A freshness
- display stale source hashes
- warn about source-less Layer B claims
- display open questions
- report graph conflicts, duplicate stable IDs, and superseded current entries

### TUI And Reports

- render ontology summaries from the store, not from prompt text
- show entities, relationships, flows, invariants, source refs, confidence, and drift state
- expose source pointers so the user or agent can open the authoritative file
- keep HTML reports as local views over the same store/export data

## Forbidden

- confirming semantic claims without source references
- applying patches from snippets only
- recording model output directly as confirmed ontology
- treating stale source hashes as current facts
- using ontology as a giant prompt dump
- treating YAML/JSON/RDF/OWL exports as more authoritative than the runtime store and ledger
- overwriting reviewed semantic claims with regenerated deterministic facts

## Relationship To Stop Gate

The stop gate checks:

- whether the requested change is connected to source files
- whether relevant invariants remain intact
- whether verification evidence exists
- whether open questions block completion
- whether the final report passed the Korean output guard

Ontology is not decoration that makes the model look smarter. It is a safety device that narrows the working space of small models.
