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
| `Qwen3.5-4B` GGUF | priority evaluation candidate | static `unverified`; local v0.30.0 adoption gate failed | Pinned Q4_K_M bytes and license sources were verified, but the recorded 64 GB macOS run added an extra instruction line and failed exact-response equality |
| `Gemma 4 E4B` GGUF | comparison candidate | static `unverified`; local v0.30.0 promotion passed | Pinned q4_0 bytes and license sources were verified; the recorded host passed the canonical local adoption gate and selected Gemma as its persistent default |
| `Qwen3.5-9B` GGUF | quality reference candidate | upstream license checked, product default postponed | RAM impact and runtime fit remain unconfirmed before measurement |

## Confirmed Source Ledger

The following source ledger separates source-recorded artifact fields from runtime claims. Source-recorded URL, size, and LFS oid are not enough to confirm local `llama.cpp` compatibility, 16 GB viability, or default product-model fit.

| Claim | Source | Checked-at | Status |
| --- | --- | --- | --- |
| The Hugging Face model card license field for `Qwen/Qwen3.5-4B` is `apache-2.0`. | https://huggingface.co/Qwen/Qwen3.5-4B | 2026-06-29 | confirmed |
| The Hugging Face model card license field for `Qwen/Qwen3.5-9B` is `apache-2.0`. | https://huggingface.co/Qwen/Qwen3.5-9B | 2026-06-29 | confirmed |
| The Hugging Face model card license field for `google/gemma-4-E4B` is `apache-2.0`, and the Google AI for Developers Gemma 4 license page publishes Apache License 2.0. | https://huggingface.co/google/gemma-4-E4B, https://ai.google.dev/gemma/apache_2 | 2026-06-29 | confirmed |
| Apache License 2.0 allows use, reproduction, modification, sublicensing, and distribution when conditions are followed, and requires license copy, modification notice, preservation of existing attribution/NOTICE, and trademark limits. | https://ai.google.dev/gemma/apache_2 | 2026-06-25 | confirmed |
| The Hugging Face API for `Qwen/Qwen3.5-4B` reports `license:apache-2.0` and the `unsloth/Qwen3.5-4B-GGUF` artifact card reports `license:apache-2.0`, base model `Qwen/Qwen3.5-4B`, and license link to the upstream Qwen license. | https://huggingface.co/api/models/Qwen/Qwen3.5-4B, https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF | 2026-07-06 | confirmed for source fields; runtime fit unverified |
| The `Qwen3.5-4B-Q4_K_M.gguf` artifact entry lists size `2740937888` and LFS oid `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`. | https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF/tree/main?recursive=1 | 2026-07-06 | source-recorded expected hash; download-byte verification still required |
| The Hugging Face API for `google/gemma-4-E4B-it-qat-q4_0-gguf` reports `license:apache-2.0`, and Google's current Gemma page publishes Apache License 2.0. | https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf, https://ai.google.dev/gemma/apache_2 | 2026-07-11 | confirmed for source fields; license is separate from host-specific runtime fit |
| The `gemma-4-E4B_q4_0-it.gguf` artifact entry lists size `5154939136` and LFS oid `e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d`. | https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf/tree/main?recursive=1 | 2026-07-06 | source-recorded expected hash; download-byte verification still required |

## Not Yet Confirmed

- real performance and stability on 16 GB RAM
- broad Korean output guard pass rate beyond the five-marker adoption smoke
- whether the recorded text-only mmproj result generalizes to multimodal use

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

Both static entries remain `unverified`; v0.30.0 permits a host-local promotion only while artifact, backend chat provenance, RAM/mmproj evidence, and canonical benchmark linkage revalidate. The recorded machine selected Gemma locally after Qwen failed exact-response equality. This does not bundle or redistribute either model and does not establish a universal default.
