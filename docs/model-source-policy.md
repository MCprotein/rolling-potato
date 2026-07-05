# Model Source Policy

Do not record model-related information as guesswork.

Model names, licenses, performance, context length, GGUF artifacts, quantization, backend compatibility, RAM requirements, multimodal support, and Korean/code quality evaluations require explicit evidence and sources.

## Principles

- If there is no source, mark it as `unconfirmed`.
- Do not confirm a model as the default recommendation without sources.
- Do not write claims such as "better", "supported", "runs", or "has this license" without evidence.
- A model name from user intent is a `candidate`, not a product fact.
- Model-related docs must record source URL, checked date, and verified claim.
- Do not mix backend names with model families. `llama.cpp` is a backend; this project currently tracks only the separately documented `Qwen` and `Gemma` candidates.

## Accepted Sources

High-priority sources:

- upstream official model card
- upstream official repository
- upstream official license document
- official artifact provider distribution page
- release asset checksum
- official `llama.cpp` compatibility docs, issue, or release note
- benchmark results run in this repository

Secondary sources:

- trusted maintainer GGUF conversion repo
- reproducible benchmark log
- release note that includes checksum

Not accepted as evidence:

- guessing from a model name
- asserting product fit from leaderboard score alone
- unsourced blog/community summary
- "probably" level inference
- inference pulled from another model family
- applying Meta Llama license/policy based only on the backend name

## Documentation Format

When confirming a model-related claim, record:

```text
Claim: <claim being confirmed>
Source: <URL>
Checked-at: <YYYY-MM-DD>
Evidence: <document field or result summary that was checked>
Status: confirmed | rejected | superseded
```

Example:

```text
Claim: <model-id> artifact license is <license-id>.
Source: <official-model-card-or-artifact-url>
Checked-at: 2026-06-22
Evidence: model card license field and artifact page license field match.
Status: confirmed
```

## Manifest Requirements

Model entries in the manifest must have at least the following fields in source-backed form:

- upstream model name
- upstream URL
- artifact URL
- artifact provider
- license
- SHA-256
- file size
- quantization
- backend compatibility
- recommended RAM evidence

If any field is missing, the model stays `candidate` or `unverified` instead of `recommended`.

Artifact URLs must be source-backed manifest entries. They must not be accepted as free-form install arguments, silently replaced with fallback URLs, or treated as verified when they point at mutable `latest`/branch targets. A URL update requires a matching checksum, file size, provider terms, and checked-at evidence update.

## Forbidden Unsourced Phrases

Do not use these without sources:

- "default model"
- "recommended model"
- "better for Korean/code"
- "multimodal support"
- "vision capable"
- "Apache-2.0"
- "runs on 16 GB"
- "supported by llama.cpp"

Use weaker wording when needed:

- "evaluation candidate"
- "candidate prioritized by user intent"
- "unconfirmed before source review"
- "not a default until benchmarked"
