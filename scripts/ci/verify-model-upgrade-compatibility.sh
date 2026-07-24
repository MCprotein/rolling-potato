#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

cargo test --locked model_upgrade_compatibility -- --test-threads=1

printf 'model upgrade compatibility ok: legacy-registry text runtime, manifest context, projector cache and v2 binding\n'
