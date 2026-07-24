#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

cargo test --locked model_upgrade_compatibility -- --test-threads=1
cargo test --locked vision_status_questions_use_runtime_facts_instead_of_model_guessing
cargo test --locked cached_model_switch_is_labeled_as_reuse_instead_of_a_new_download
cargo test --locked setup_reuses_a_cached_model_without_claiming_a_new_download
cargo test --locked setup_options_distinguish_local_model_cache_from_lazy_projector_download

printf 'model upgrade compatibility ok: legacy registry, context, projector binding, vision status and cached model switching\n'
