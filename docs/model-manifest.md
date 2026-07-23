# Model Manifest

The model manifest is the trust boundary through which `rpotato` understands installable models and local model state.

## Goals

- Identify model artifacts explicitly.
- Show download size and license before download.
- Verify SHA-256 after download.
- Check backend compatibility.
- Separate prompt/runtime options per model.
- Confirm model-related claims only through sources.

## Location

Initial manifest source can live in the repository or a GitHub Release asset. Installable artifact download URLs are manifest data, not Rust source-code constants.

Local cache follows platform directory rules:

- macOS: `~/Library/Application Support/rpotato/manifests/`
- Windows: `%LOCALAPPDATA%\rpotato\manifests\`
- Linux: `~/.local/share/rpotato/manifests/`

Exact paths are confirmed after the Rust scaffold decides whether to use a `directories`-style crate.

## Download URL Management

The installable download URL field is `artifactUrl`.

- Production download URLs are pinned in a versioned model manifest, not hardcoded in Rust code.
- The current Rust static candidate table is only an early scaffold for candidate metadata. It may carry source-backed candidate artifact facts for CLI inspection, but it is not the production manifest distribution surface.
- Entries with `status != "verified"` are not installable even when an artifact URL, checksum, and file size are already known.
- `rpotato model install` must not accept an arbitrary user-provided URL as a model source.
- Avoid `latest`, branch, or moving redirect URLs as the source of trust. When a provider supports immutable revision or release URLs, `artifactUrl` must point to that concrete artifact.
- Every `artifactUrl` must be recorded with `artifactProvider`, `artifactTermsUrl`, `artifactName`, `sha256`, `sizeBytes`, and source/checked-at evidence.
- If the URL changes, the checksum, size, provider evidence, and manifest entry must be updated together.

## Current Source-Backed Artifact Candidates

These entries are candidate facts, not install-ready defaults. The Hugging Face LFS `oid` values are recorded as the expected SHA-256 values and must be rechecked against downloaded bytes before registry registration.

| Model ID | Artifact provider | Artifact | Revision | Quantization | Size bytes | SHA-256 | Source status |
| --- | --- | --- | --- | --- | ---: | --- | --- |
| `qwen3.5-4b` | `unsloth/Qwen3.5-4B-GGUF` | `Qwen3.5-4B-Q4_K_M.gguf` | `e87f176479d0855a907a41277aca2f8ee7a09523` | `Q4_K_M` | `2740937888` | `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4` | Static `unverified`; recorded host-local smoke/RAM/mmproj evidence exists, but exact-response equality failed and local promotion is invalid |
| `gemma-4-e4b` | `google/gemma-4-E4B-it-qat-q4_0-gguf` | `gemma-4-E4B_q4_0-it.gguf` | `bb3b92e6f031fa438b409f898dd9f14f499a0cb0` | `QAT q4_0` | `5154939136` | `e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d` | Static `unverified`; recorded host-local smoke/RAM/mmproj evidence passed and supports a revalidated local promotion/default selection |

The main GGUF contains the language model. A model-specific `mmproj` contains the
visual encoder/projector that maps image features into that language model's
embedding space. The two artifacts are not interchangeable across models or
revisions. Vision readiness therefore requires the independently pinned projector
bytes as well as the main artifact; text readiness does not.

| Model ID | Vision artifact | Revision | Size bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| `qwen3.5-4b` | `unsloth/Qwen3.5-4B-GGUF/mmproj-F16.gguf` | `e87f176479d0855a907a41277aca2f8ee7a09523` | `672423616` | `cd88edcf8d031894960bb0c9c5b9b7e1fea6ebee02b9f7ce925a00d12891f864` |
| `gemma-4-e4b` | `google/gemma-4-E4B-it-qat-q4_0-gguf/gemma-4-E4B-it-mmproj.gguf` | `bb3b92e6f031fa438b409f898dd9f14f499a0cb0` | `991551904` | `c6398448d84a4836fdedf58f9775979e69ae0cc4dfdf4d697b5597693a555b12` |

Registry schema v2 records `visionStatus` and the verified projector path, hash,
and size. A schema v1 registry is read as legacy text-only state and is never
silently upgraded to a vision claim. Explicit model preparation writes v2.

Sources checked on 2026-07-23:

- https://huggingface.co/api/models/Qwen/Qwen3.5-4B
- https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF
- https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF/tree/e87f176479d0855a907a41277aca2f8ee7a09523?recursive=1
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-unquantized
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf/tree/bb3b92e6f031fa438b409f898dd9f14f499a0cb0?recursive=1
- https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md

## Schema Draft

```json
{
  "schemaVersion": 1,
  "models": [
    {
      "id": "qwen3.5-4b-q4-k-m",
      "displayName": "Qwen3.5 4B Q4_K_M",
      "status": "candidate",
      "upstreamModel": "TODO",
      "upstreamUrl": "TODO",
      "artifactName": "TODO.gguf",
      "artifactProvider": "TODO",
      "artifactTermsUrl": "TODO",
      "format": "gguf",
      "backend": "llama.cpp",
      "quantization": "TODO",
      "recommendedRamGb": null,
      "recommendedRamEvidence": "TODO",
      "contextLength": null,
      "license": "TODO",
      "licenseSource": "TODO",
      "licenseCheckedAt": "TODO",
      "artifactUrl": "TODO",
      "sha256": "TODO",
      "sizeBytes": null,
      "backendCompatibility": "TODO",
      "publishedBenchmark": {
        "source": "TODO",
        "checkedAt": "TODO",
        "claimStatus": "source-listed-unreproduced",
        "parityStatus": "not-run"
      }
    }
  ]
}
```

This example shows schema shape only. `TODO` and `null` are not product facts; real manifests must fill values after source review under [model-source-policy.md](model-source-policy.md).

Current CLI surface:

- `rpotato model list`
- `rpotato model manifest`
- `rpotato model inspect <id>`
- `rpotato model registry`
- `rpotato model download-plan <id>`
- `rpotato model eval-plan <id>`
- `rpotato model benchmark-plan <id>`
- `rpotato model fetch-candidate <id> --for-evaluation`
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model promote <id> --evidence <file>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`
- `rpotato model default [<id>]`

Candidate states:

- `candidate`: upstream model/source/license is tracked, but installable GGUF artifact is not selected yet
- `unverified`: GGUF artifact candidate exists, but one or more install-promotion checks are missing, such as byte-level checksum verification, provider/source review, backend smoke, RAM-fit evidence, mmproj need review, or benchmark evidence
- `verified`: source, license, artifact URL, provider terms, SHA-256, file size, backend compatibility, and RAM evidence are all present for pre-install display

`model download-plan` renders source, license source, artifact provider, artifact terms, file size, SHA-256, resume path, and final path before real download.

`model eval-plan <id>` is read-only. It reports whether source-backed artifact fields exist, whether the expected app-data artifact file is missing or size/SHA-256 verified, and which local smoke/benchmark step is next.

`model benchmark-plan <id>` is read-only. It separates public benchmark reproduction conditions from local product benchmark fixtures and blocks score parity claims until artifact, quantization, backend, context length, prompt/template, dataset version, and scoring method are recorded together.

`model fetch-candidate <id> --for-evaluation` is the only path that downloads `unverified` source-backed artifacts. It requires the explicit evaluation flag, writes only app-managed partial/final artifact files, verifies file size and SHA-256 before success, records a ledger event, and does not create `models/registry/<model-id>.json`.

`model verify-file` streams a local file, computes SHA-256, and compares it to the expected hash. Success and failure both record ledger events, and failure must block registry registration.

`model promote <id> --evidence <file>` validates local promotion evidence before any registry registration. The command requires:

- app-managed artifact file size and SHA-256 to match the source-backed manifest
- a `backend.chat.completed` event whose backend binary, model artifact, context, sampling, mmproj state, OS, and architecture provenance match the candidate
- `ramFit=observed-within-local-host`, measured `peakRssBytes`, and `recommendedRamGb` equal to `ceil(peak RSS GiB) + 2 GiB`
- explicit `mmproj` evidence: `not-required-text-only`, `not-required`, or `required`
- a SQLite row whose fixture ID, fixture SHA-256, prompt SHA-256, benchmark name, and dataset reference match the release-pinned canonical adoption contract; whose requested/effective max tokens are both 192; whose quantization resolves from the source-backed model manifest; and whose `model_run_id` directly names that chat event, is `claim_state=measured-locally`, has `local_pass=true`, uses the candidate backend, and carries matching `peak_rss_bytes`

Promotion evidence JSON schema:

```json
{
  "schemaVersion": 1,
  "modelId": "gemma-4-e4b",
  "artifactSha256": "e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d",
  "artifactSizeBytes": 5154939136,
  "backendId": "llama.cpp",
  "backendVersion": "b9878",
  "backendSmokeEventId": "event-...",
  "ramFit": "observed-within-local-host",
  "recommendedRamGb": 8,
  "peakRssBytes": 5521932288,
  "mmproj": "not-required-text-only",
  "benchmarkRunId": "benchmark-event-...",
  "recordedAt": "2026-07-11T00:00:00Z"
}
```

Successful promotion writes normalized evidence to `models/evidence/<model-id>.promotion.json`. This is still local evidence, not a public benchmark parity claim.

`model cleanup-failed` targets only app-managed partial/failed artifact paths under app data `downloads/` and `models/`. Deletion runs only with explicit `--delete`; default verification and doc smoke use `--dry-run`.

`model install` registers a model only when either the source-backed manifest entry is statically `verified` or `models/evidence/<model-id>.promotion.json` revalidates against local artifact, ledger, RAM/mmproj, and benchmark evidence. Registry entries are written to `models/registry/<model-id>.json`.

Interactive first-run setup is a separate, explicitly user-selected execution lane. It may write a `source-backed-manifest` registry entry only after the pinned source, artifact size and SHA-256, confirmed license, and backend-compatibility source revalidate. Such an entry does not assert RAM fit, capability quality, or benchmark parity, and does not satisfy the stricter `model install` promotion gate.

`model default <id>` selects a registered model whose artifact and evidence status still revalidate. Accepted evidence is either current local promotion evidence or the exact restricted `source-backed-manifest` setup state above. `model default` shows the selection. `backend start` may omit `--model`; it then revalidates and resolves the persistent default or fails closed.

## v0.30.0 Local Adoption Record

On 2026-07-11, both pinned artifacts ran the same hash-pinned five-line local adoption contract on a MacBook Pro with Apple M5 Pro and 64 GB RAM, `llama.cpp b9878`, context 4096, temperature 0.1, and top-p 0.8. The latest Gemma run recorded `1686 ms`, `61.6845 tokens/s`, peak RSS `5521932288` bytes, and passed `3/3`. Qwen recorded `1680 ms`, `61.9048 tokens/s`, peak RSS `3296378880` bytes, but scored `2/3` because it echoed the instruction line before all 5 required markers. Both matched no forbidden marker.

Gemma was locally promoted, registered, and selected as the persistent default because it passed exact-response equality. Qwen's lower RSS did not override the failed instruction contract. This is not a universal model ranking, 16 GB evidence, or public benchmark parity. Model weights remain outside repository and release assets.

## Required Verification

Model install must:

1. check manifest schema version
2. check OS/backend compatibility
3. show download size to the user
4. require user approval before download
5. verify SHA-256 after download
6. avoid registry registration after verification failure
7. require local promotion evidence before installing unverified source-backed candidates

## Forbidden

- installing models without checksum
- recommending models without license
- source-less license, RAM, context length, or backend compatibility claims
- automatic download without user approval
- bundling model weights into the `rpotato` release binary
- hardcoding installable artifact URLs in Rust source
- silent fallback to arbitrary URLs
- treating `latest` or moving URLs as verified artifacts

## Open Questions

- whether manifest signing belongs in MVP
- whether artifact mirrors are allowed
- whether quantization-specific prompt presets belong in the manifest
- how far provider trust policy should be automated
