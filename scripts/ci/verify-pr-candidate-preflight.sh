#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

scripts/release/verify-toolchain-pins.sh
cargo fmt --all -- --check
scripts/ci/verify-model-upgrade-compatibility.sh
cargo test --locked --test architecture_contract -- --test-threads=1
cargo clippy --locked --all-targets --all-features -- -D warnings
bash scripts/release/test-release-workflow-contract.sh

printf 'PR candidate preflight ok: tool-pins format model-upgrade architecture clippy workflow-contract\n'
