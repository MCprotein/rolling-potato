# Ontology Runtime

Ontology is the core runtime memory of `rolling-potato`.

Small models cannot reliably reconstruct project structure, terminology, ownership, workflows, and invariants from prompts every turn. The runtime core manages this meaning structure as ontology so it can reduce model freedom and prevent repeated mistakes.

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

## Minimal Schema

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

Each semantic entry must include:

- `id`
- `kind`
- `statement`
- `sourceRefs`
- `confidence`
- `status`
- `updatedAt`

## Runtime Use

### `rpotato init`

- create project identity
- seed Layer A facts
- prepare project-local `.rpotato/` state layout
- display ontology gap diagnostics

### `rpotato run`

- find ontology entries related to the user request
- promote source pointers to original-file reads
- pass only the minimum required entries to the prompt compiler
- check whether action candidates violate invariants
- record apply/verification evidence in the ledger

### `rpotato doctor`

- check Layer A freshness
- display stale source hashes
- warn about source-less Layer B claims
- display open questions

## Forbidden

- confirming semantic claims without source references
- applying patches from snippets only
- recording model output directly as confirmed ontology
- treating stale source hashes as current facts
- using ontology as a giant prompt dump

## Relationship To Stop Gate

The stop gate checks:

- whether the requested change is connected to source files
- whether relevant invariants remain intact
- whether verification evidence exists
- whether open questions block completion
- whether the final report passed the Korean output guard

Ontology is not decoration that makes the model look smarter. It is a safety device that narrows the working space of small models.
