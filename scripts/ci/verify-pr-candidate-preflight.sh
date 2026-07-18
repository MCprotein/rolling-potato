#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

scripts/release/verify-toolchain-pins.sh
cargo fmt --all -- --check
cargo test --locked --test architecture_contract migration_map_recursively_covers_every_governed_file_and_exact_slice -- --exact --test-threads=1
cargo clippy --locked --all-targets --all-features -- -D warnings
bash scripts/release/test-package-manager-manifests.sh
scripts/release/verify-package-manager-prerequisites.sh
bash scripts/release/test-package-manager-workflow-contract.sh
bash scripts/release/test-release-workflow-contract.sh

printf 'PR candidate preflight ok: tool-pins format architecture clippy package-managers workflow-contract\n'
