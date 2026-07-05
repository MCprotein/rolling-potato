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
- The current Rust static candidate table is only an early scaffold for candidate metadata; entries with `artifactUrl: null` or missing checksum are not installable.
- `rpotato model install` must not accept an arbitrary user-provided URL as a model source.
- Avoid `latest`, branch, or moving redirect URLs as the source of trust. When a provider supports immutable revision or release URLs, `artifactUrl` must point to that concrete artifact.
- Every `artifactUrl` must be recorded with `artifactProvider`, `artifactTermsUrl`, `artifactName`, `sha256`, `sizeBytes`, and source/checked-at evidence.
- If the URL changes, the checksum, size, provider evidence, and manifest entry must be updated together.

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
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`

Candidate states:

- `candidate`: upstream model/source/license is tracked, but installable GGUF artifact is not selected yet
- `unverified`: GGUF artifact candidate exists, but checksum, provider terms, backend compatibility, or RAM evidence is missing
- `verified`: source, license, artifact URL, provider terms, SHA-256, file size, backend compatibility, and RAM evidence are all present for pre-install display

`model download-plan` renders source, license source, artifact provider, artifact terms, file size, SHA-256, resume path, and final path before real download.

`model verify-file` streams a local file, computes SHA-256, and compares it to the expected hash. Success and failure both record ledger events, and failure must block registry registration.

`model cleanup-failed` targets only app-managed partial/failed artifact paths under app data `downloads/` and `models/`. Deletion runs only with explicit `--delete`; default verification and doc smoke use `--dry-run`.

`model install` blocks entries that are not `verified` and records a ledger event. Current implementation does not perform real downloads. The local registry is prepared as a boundary that records only verified artifacts at `models/registry/<model-id>.json`.

## Required Verification

Model install must:

1. check manifest schema version
2. check OS/backend compatibility
3. show download size to the user
4. require user approval before download
5. verify SHA-256 after download
6. avoid registry registration after verification failure

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
