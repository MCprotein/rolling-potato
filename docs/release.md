# Release

This document is the draft release policy for `rolling-potato`.

## Release Principles

- Use GitHub Releases as the default distribution channel.
- Do not include model weights in the `rpotato` release binary.
- Do not include external plugin packages in the `rpotato` release binary.
- Do not use external plugin marketplaces, registries, catalogs, or mirrors as distribution channels.
- Distribute each binary with a checksum.
- Release notes include supported OS, backend version, and known issues.

## Version Policy

`rolling-potato` follows SemVer in spirit, but pre-1.0 versions must stay
practical. Do not bump the major version for every new feature.

Before MVP and before the first stable contract, use `0.x.y`.

- `0.x.0`: coherent feature or milestone release
- `0.x.y`: bugfix, documentation, packaging, manifest, policy, or test fix

During `0.x`, breaking changes are allowed only when they move the project
toward the MVP runtime contract. They should normally bump the minor version and
must be called out in release notes as `Breaking before 1.0`. Do not use `1.0`
language until the stable user-facing contract is ready.

Examples:

- `0.1.1`: fix `doctor`, update docs, update release notes, fix packaging
- `0.2.0`: add first `rpotato run` context-aware model-response skeleton, model-run metrics, and non-executing model action parsing
- `0.3.0`: add patch diff display and approval gate
- `0.4.0`: add approved patch apply and verification command flow
- `0.5.0`: add TUI beta surface

After `1.0.0`, follow SemVer strictly:

- Patch: backwards-compatible fixes, docs, packaging, manifest updates,
  security hardening, diagnostics, and test fixes
- Minor: backwards-compatible features, new commands, new optional flags, new
  adapters, new TUI panels, new policy checks that fail closed without removing
  stable APIs
- Major: incompatible changes to stable public contracts

Stable public contracts include:

- CLI command names, flags, exit semantics, and documented output schemas
- config file schema
- model/backend manifest schema
- plugin manifest schema and adapter behavior
- ledger, session, evidence, and SQLite migration compatibility
- TUI workflows that users depend on for approvals, resume, and monitoring

Major releases should be batched and rare. Prefer deprecation warnings, aliases,
or migration commands before removing stable behavior. Experimental commands,
unstable schemas, or features explicitly marked `preview` do not require a major
bump until they are promoted to the stable contract.

Documentation-only changes do not require a version bump unless a release
artifact is being cut. Model metadata corrections should use patch releases when
they only update source-backed manifest facts; they must not imply a new model
recommendation without benchmark/runtime evidence.

## Release Branch Policy

Every shipped version must use a short-lived version branch.

Branch naming:

- stable release: `release/vX.Y.Z`
- preview release: `release/vX.Y.Z-alpha.N`, `release/vX.Y.Z-beta.N`, or
  `release/vX.Y.Z-rc.N`

Rules:

1. Create the release branch from `main`.
2. Bump `Cargo.toml` to the exact version represented by the branch name.
3. Finish release notes, docs, manifests, and release checks on that branch.
4. Merge the release branch into `main` only after the release checklist passes.
5. Tag the merge commit as `vX.Y.Z` or the matching prerelease tag.
6. Push the tag while preserving the matching remote release branch.
7. Create the GitHub Release from that tag. The `release-binaries` workflow
   builds and uploads supported binary assets, downloads the published set,
   verifies the exact 11 regular non-symlink assets and all checksums, and only
   then deletes the merged matching remote release branch.
8. Verify the published workflow and assets, then delete the local release branch.

`main` is the integration branch. Release branches are not long-running support
branches and must not accumulate unrelated post-release work.

## Release Policy Enforcement

The repository includes a release policy check:

```sh
scripts/release/verify-release-policy.sh
```

The check enforces:

- release branch name matches `Cargo.toml` package version
- release tag matches `Cargo.toml` package version
- pull requests that change the package version, and branches named `release/*`,
  must use the matching `release/v...` branch; ordinary feature and docs PRs do
  not impersonate release work
- release tags are on `origin/main`
- release tags require the matching remote release branch to be a direct
  ancestor or an exact tree-equivalent squash merge when it still exists
- tag-policy validation never deletes the remote release branch; cleanup has one
  owner in the successful published-asset verification DAG

Required GitHub repository settings:

- protect `main`
- require pull requests before merging to `main` for release work
- require the `release-policy` status check before merging
- disallow force pushes on `main`
- allow GitHub Actions `contents: write` for this repository so the release
  workflow can delete merged `release/v...` branches

Repository settings are outside the source tree, so they must be configured in
GitHub by a maintainer. The workflow and script provide the repo-local
enforcement surface.

## Binary Release Workflow

`release-binaries` builds release assets when a GitHub Release is published.
It can also be run manually with a `release_tag` input for workflow validation.

Current release asset set:

- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `rpotato-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256`
- `rpotato-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
- `rpotato-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip.sha256`
- `rpotato-vX.Y.Z-checksums.txt`

The workflow first runs a `release test gate` job on `ubuntu-24.04` with
`cargo test --locked -- --test-threads=1` and
`scripts/release/verify-release-target-matrix.sh`. The serialized gate includes
the backend sidecar timeout/stale-record lifecycle tests.
Target build jobs depend on that gate, then build the release binary, run
`scripts/release/verify-release-binary-smoke.sh` against the built binary, and
upload the archive and checksum to the GitHub Release. Windows jobs also run
`scripts/release/verify-uninstall-smoke.sh` so `--keep-cache` and
`--purge-cache` dry-run plans stay visible and non-destructive in packaged
binaries. `rpotato doctor` is the release-smoke command because it reports
package version, target OS/arch, binary suffix, backend/model/cache summaries,
and does not download models, install backends, start sidecars, or require
network access.

After all target artifacts are built, the `checksums` job publishes
`rpotato-vX.Y.Z-checksums.txt` by concatenating the per-asset `.sha256` files.
All checksum entries must point to release asset basenames, not build-directory
paths, and must use LF line endings so the aggregate file is verifiable on Unix
and Windows. `scripts/release/verify-release-target-matrix.sh` keeps the workflow
matrix pinned to the supported release targets.

The workflow pins the immutable commits for the current stable Node.js 24
action releases checked on 2026-07-13: `actions/checkout` v7.0.0,
`actions/upload-artifact` v7.0.1, and `actions/download-artifact` v8.0.1.
The version comments remain beside each full commit SHA in the workflow.
`scripts/release/verify-toolchain-pins.sh` enforces the coordinated Rust,
Action-SHA, and GA runner pins in pull-request and release test gates.

The runner labels are pinned to the current GA target images:

- macOS Apple Silicon: `macos-26` / `aarch64-apple-darwin`
- macOS Intel: `macos-26-intel` / `x86_64-apple-darwin`
- Linux x86_64: `ubuntu-24.04` / `x86_64-unknown-linux-gnu`
- Linux ARM64: `ubuntu-24.04-arm` / `aarch64-unknown-linux-gnu`
- Windows x86_64: `windows-2025` / `x86_64-pc-windows-msvc`

GitHub's runner-images reference listed these GA labels when checked on
2026-07-13:
https://github.com/actions/runner-images

## Artifact Targets

Current archive targets:

- macOS Apple Silicon
- macOS Intel
- Linux x86_64
- Linux ARM64
- Windows x86_64

Prepared package-manager targets:

- Homebrew: macOS arm64/x64 and Linux arm64/x64
- Scoop: Windows x64
- winget: Windows x64 portable ZIP

## Package Manager Distribution

Package-manager manifests are derived artifacts. The repository does not
hand-maintain release URLs or checksums separately for each channel.

The release workflow:

1. verifies the exact 11-file GitHub Release asset set;
2. reads the five archive hashes from the verified aggregate checksum file;
3. generates one Homebrew formula, one Scoop manifest, and the three-file
   winget manifest set;
4. independently verifies their exact paths, versions, URLs, archive names, and
   hashes;
5. validates clean install, previous-stable-to-current upgrade, the version
   reported by `rpotato doctor`, package-manager uninstall, and command absence
   on six native lanes;
6. uploads the current generated trees as a publication-candidate workflow
   artifact.

The package-manager workflow does not add files to the GitHub Release. The
existing five archives, five sidecar checksums, and one aggregate checksum
remain the complete release-asset contract.

### Channel status

Use these states consistently:

- `Generated`: deterministic manifests were produced from a verified aggregate
  checksum.
- `Validated`: the static verifier and all applicable native lifecycle lanes
  passed.
- `Published`: an authorized external write completed and its public URL or
  upstream review status was recorded.
- `Unpublished`: no live external channel is claimed, even if generated and
  validated artifacts exist.

As of 2026-07-18, the v0.40.0 release artifact is `Generated` and `Validated`
across all six native lifecycle lanes. The Homebrew tap is `Published` at
[MCprotein/homebrew-rpotato](https://github.com/MCprotein/homebrew-rpotato)
commit `bf50499674dcbf46ce7e36260a8a6b3cf0c6b49e`, and the Scoop bucket is
`Published` at
[MCprotein/scoop-rpotato](https://github.com/MCprotein/scoop-rpotato) commit
`2e881e23456ae818d00ae63a1059bd870fc914de`. The winget manifest is generated
and validated, but it is `Unpublished`: no upstream submission is active and
no community package is published.

### Integrity and pinned prerequisites

`rpotato-vX.Y.Z-checksums.txt`, accepted by
`scripts/release/verify-release-assets.sh`, is the only source for hashes placed
in package-manager manifests. All download URLs remain immutable versioned
HTTPS URLs under the matching GitHub Release. Qualification, release, and
recovery preparation also require both selected GitHub Releases to match their
requested stable tags, be published, and be neither drafts nor prereleases.

The native validation prerequisites recorded on 2026-07-18 are:

- `Homebrew/actions/setup-homebrew` commit
  `df4b09108a1de9d6f995fe68f302b3f68bd6d2ef`;
- Scoop source and schema commit
  `b588a06e41d920d2123ec70aee682bae14935939`;
- winget client release `v1.29.280`, with the bundle and dependency archive
  SHA-256 values enforced by
  `scripts/release/verify-package-manager-prerequisites.sh`;
- winget manifest schema `1.12.0`.

Workflow logs record the actual manager/client versions used by each native
lane. Unpinned remote bootstrap pipelines are forbidden.

### Qualification and recovery

Before the v0.40.0 tag, manually dispatch
`package-manager-distribution` with:

```text
mode=qualification
previous_tag=v0.38.0
current_tag=v0.39.0
```

Qualification uses already published releases to prove the pinned setup,
manifest formats, and install/upgrade/uninstall lifecycle on four Homebrew
lanes plus isolated Scoop and winget lanes. It performs no Cargo build, release
upload, tag creation, external publication, or branch cleanup.

For a package-manager-only failure after publishing a stable tag, dispatch
`mode=recovery` with only `current_tag`. Recovery derives the greatest
ancestral stable predecessor, re-verifies both exact release asset sets, and
reruns only manifest preparation and the six native lifecycle lanes. It never
overwrites release assets or creates a new patch tag. The matching release
branch is deleted only after all lanes pass; any failure preserves the branch.

### Install and removal boundary

Once an external channel is `Published`, the commands documented in the
bilingual READMEs install, update, and remove the package-manager-owned binary.
Package-manager removal does not remove `rpotato` application data, models, or
cache. Run `rpotato uninstall --dry-run` before removing the executable to
inspect that separate cleanup plan.

### Release evidence

The release record must include:

- candidate commit SHA and exact-HEAD CI run;
- exact 11-asset verification;
- pre-tag v0.38.0 to v0.39.0 qualification run;
- pinned prerequisites and observed manager/client versions;
- resolved previous stable tag and ancestry decision;
- generated publication artifact ID and hash;
- clean-install and upgrade conclusions for all six native lanes;
- normal deployment or same-tag recovery run;
- Homebrew tap URL/commit/status;
- Scoop bucket URL/commit/status;
- winget pull-request/merge/package status;
- release-branch cleanup result.

Any external entry without confirmed evidence remains `Unpublished` or
`Pending external review`; it must not be reported as complete.

## Release Checklist

Before release:

1. `cargo fmt --check`
2. `cargo test`
3. `cargo clippy --all-targets -- -D warnings`
4. `rpotato doctor` smoke test
5. model manifest checksum verification
6. Korean output guard tests
7. destructive command policy tests
8. if plugin adapter is included, local-import-only and remote-source rejection smoke tests
9. release notes
10. repository toolchain pin guard
11. release target matrix guard
12. binary checksums
13. after publishing the GitHub Release, confirm the `release-binaries` workflow
    uploaded all target archives, matching `.sha256` files, and the aggregate
    `checksums.txt` file
14. for v0.40.0 or later package-manager releases, record the successful pre-tag
    qualification run
15. confirm all six package-manager lifecycle lanes passed or explicitly record
    each external channel as `Unpublished`

Use [release-notes-template.md](release-notes-template.md) for new release note
entries.

## Model Manifest Distribution

Model manifest can be distributed through the repo or a release asset.

The manifest needs at least:

- model id
- upstream model
- quantization
- GGUF artifact URL
- SHA-256
- license
- recommended RAM
- context length

Whether model weights are redistributed directly is decided only after model-specific license and artifact provider policy review.

## Plugin Distribution Boundary

`rpotato` releases distribute only plugin adapters. They do not distribute external plugin packages.

Allowed:

- local plugin directory import
- Codex/Claude Code plugin manifest parsers
- inspect/validate/enable/disable/remove commands

Not allowed:

- plugin marketplace integration
- plugin registry/catalog integration
- third-party plugin package mirror
- remote URL plugin install
- official Codex/Claude Code compatibility claim
