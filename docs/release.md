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

Before MVP, use `0.x.y`.

- `0.x.0`: feature-level release
- `0.x.y`: bugfix, documentation, or packaging fix

After stabilization, follow SemVer.

## Artifact Targets

Initial targets:

- macOS Apple Silicon
- macOS Intel
- Windows x86_64

Later targets:

- Linux x86_64
- Linux ARM64

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
10. binary checksums

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
