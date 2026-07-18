#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

cargo test --release --locked --test workflow_performance \
  completed_agent_subagent_and_team_workflows_stay_within_budgets \
  -- --exact --nocapture --test-threads=1
