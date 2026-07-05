# Model Licenses

The `rolling-potato` code license and model licenses are separate.

- Project code: Apache-2.0
- Model weights: must be recorded after checking each upstream model and GGUF artifact provider license

This document covers only `Qwen` and `Gemma` candidates. `llama.cpp` is an inference backend name, not a model candidate. Using `llama.cpp` does not bring Meta Llama-family models or their licenses into this project.

## Principles

- Do not confirm a default recommended model before checking model license.
- GGUF conversions require both upstream model license and artifact provider terms.
- The manifest must show license per model.
- If redistribution is unclear, do not bundle the model directly into `rpotato` artifacts.
- Do not present model or GGUF artifacts as if they are owned like project code.
- All model claims follow [model-source-policy.md](model-source-policy.md).

## Bundling / Redistribution Decision

Conclusion: models can be bundled or connected to the install flow only within the original license conditions. Model weights or conversions must not be treated as exclusive `rolling-potato` property.

Current policy:

- Do not commit model weights to the project source repository.
- Prefer manifest-based downloads for default distribution.
- Before model download, show license, source, artifact provider, file size, and checksum.
- Bundle distribution is allowed only after upstream license, GGUF artifact provider terms, NOTICE/attribution requirements, and checksum are all confirmed.
- The `rolling-potato` Apache-2.0 license applies to project code; included third-party models and artifacts keep their original licenses.
- If modified model files or conversions are distributed, document the modification and original source.

## Initial Candidates

| Candidate | Role | Status | Notes |
| --- | --- | --- | --- |
| `Qwen3.5-4B` GGUF | priority evaluation candidate | upstream license checked, GGUF not selected | exact artifact, hash, and runtime fit still need review |
| `Gemma 4 E4B` GGUF | comparison candidate | upstream license checked, GGUF not selected | exact artifact, hash, and runtime fit still need review |
| `Qwen3.5-9B` GGUF | quality reference candidate | upstream license checked, product default postponed | RAM impact and runtime fit remain unconfirmed before measurement |

## Confirmed Upstream Sources

The following confirms upstream model information only. It does not confirm GGUF conversion provider, checksum, file size, `llama.cpp` compatibility, 16 GB viability, or default product-model fit.

| Claim | Source | Checked-at | Status |
| --- | --- | --- | --- |
| The Hugging Face model card license field for `Qwen/Qwen3.5-4B` is `apache-2.0`. | https://huggingface.co/Qwen/Qwen3.5-4B | 2026-06-29 | confirmed |
| The Hugging Face model card license field for `Qwen/Qwen3.5-9B` is `apache-2.0`. | https://huggingface.co/Qwen/Qwen3.5-9B | 2026-06-29 | confirmed |
| The Hugging Face model card license field for `google/gemma-4-E4B` is `apache-2.0`, and the Google AI for Developers Gemma 4 license page publishes Apache License 2.0. | https://huggingface.co/google/gemma-4-E4B, https://ai.google.dev/gemma/apache_2 | 2026-06-29 | confirmed |
| Apache License 2.0 allows use, reproduction, modification, sublicensing, and distribution when conditions are followed, and requires license copy, modification notice, preservation of existing attribution/NOTICE, and trademark limits. | https://ai.google.dev/gemma/apache_2 | 2026-06-25 | confirmed |

## Not Yet Confirmed

- default recommended model
- GGUF artifact URL to use
- GGUF artifact provider license/terms
- SHA-256 and file size
- actual `llama.cpp` compatibility
- real performance and stability on 16 GB RAM
- Korean output guard pass rate

## Artifact Selection Checklist

- check upstream model card
- check license
- check GGUF converter trust
- record SHA-256
- record file size
- check context length
- check `llama.cpp` compatibility
- check Windows execution

## Information To Document

When a model is added to the manifest, record:

- upstream model name
- upstream URL
- artifact URL
- license
- redistribution policy
- quantization
- SHA-256
- evaluation result

## Open Issue

The exact `Qwen3.5-4B` artifact has not been selected yet. Run the evaluation in [model-eval.md](model-eval.md) and [benchmarks.md](benchmarks.md) before selection.
