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

## Distribution Policy

The project distributes binaries only through the
[rolling-potato GitHub Releases](https://github.com/MCprotein/rolling-potato/releases)
page. The release workflow builds the five supported archives, publishes one
checksum beside each archive, publishes the aggregate checksum file, and
verifies the exact 11-file asset set before release-branch cleanup.

Homebrew, Scoop, winget, apt, rpm, npm, container registries, and other external
package channels are not project distribution surfaces. The v0.40.0
package-manager experiment remains part of the historical release notes, but
its manifest generators, fixtures, validation workflow, and external channels
were retired on 2026-07-19. Future releases must not write to or advertise
another package repository.

To install or upgrade, download the desired versioned archive from GitHub
Releases and verify it against the matching sidecar checksum or
`rpotato-vX.Y.Z-checksums.txt`. From v0.42.0, run the extracted executable with
`rpotato install`, then invoke the installed binary's `rpotato init`. The
installer writes only the user-local binary directory and one managed PATH
block in the detected zsh/bash/fish profile, or the Windows user PATH. New
terminals inherit it automatically, and the report includes a current-terminal
activation command.

From v0.44.0, an existing managed installation can check and apply the latest
stable release with `rpotato update --check` and `rpotato update`, or with
`/update` after the startup notice in the TUI. The startup request has a short
timeout and a six-hour cache; failure is silent and never blocks the TUI. The
apply path accepts only the five supported platform mappings, constructs URLs
inside this repository's GitHub Releases, verifies the matching `.sha256`
sidecar, rejects unsafe or duplicate archive paths, and replaces only an owned
managed installation. Windows performs the final replacement after process
exit and keeps a rollback copy until the move succeeds.

`rpotato install --clean --dry-run` shows the exact binary/PATH change and the
global application-data and current-project `.rpotato` targets. Deletion requires
`rpotato install --clean --yes` and is blocked while a managed backend or
generation is active, when liveness cannot be verified, or while runtime state
is being published. It never treats package-manager repositories as an
installation source.

`rpotato uninstall --clean --dry-run` lists the installed binary, owned PATH
registration, global application data, and current-project state without
mutation. `rpotato uninstall --clean --yes` removes those managed targets under
the same runtime guard. The extracted invocation binary and source repository
remain user-owned; Windows self-deletion completes after process exit.

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

`config/release-targets.tsv` is the canonical target list used by the Rust
updater and release-asset verification. The release matrix guard rejects a
workflow tuple or asset set that diverges from it.

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
