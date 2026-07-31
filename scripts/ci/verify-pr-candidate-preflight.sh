#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

scripts/release/verify-toolchain-pins.sh
scripts/ci/verify-web-browser-docs.sh
cargo fmt --all -- --check
scripts/ci/verify-model-upgrade-compatibility.sh
cargo test --locked --bin rpotato app::workflow_adapter::state::tests::current_snapshot::current_state_lease_releases_ledger_guard_before_loading_active_workflow -- --exact --test-threads=1
cargo test --locked --bin rpotato app::intent_adapter::tests::read_only_mode_plans_source_inspection_without_approval -- --exact --test-threads=1
cargo test --locked --test workflow_performance completed_agent_subagent_and_team_workflows_stay_within_budgets -- --exact --test-threads=1
cargo test --locked --test subagent_lifecycle cli_subagent_lifecycle_is_bounded_deterministic_and_secret_safe -- --exact --test-threads=1
cargo test --locked --test surfaces native_terminal::pty_drop_escalates_when_child_cannot_handle_sigterm -- --exact --test-threads=1
cargo test --locked --test architecture_contract -- --test-threads=1
cargo clippy --locked --all-targets --all-features -- -D warnings
bash scripts/release/test-release-workflow-contract.sh

printf 'PR candidate preflight ok: tool-pins web-browser-docs format model-upgrade subagent-lifecycle native-terminal architecture clippy workflow-contract\n'
