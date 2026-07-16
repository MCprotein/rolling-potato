#!/usr/bin/env bash
set -euo pipefail

run_proof() {
  local proof_id="$1"
  shift
  printf 'release proof start: %s\n' "$proof_id"
  cargo test --locked "$@" -- --exact --test-threads=1
  printf 'release proof ok: %s\n' "$proof_id"
}

run_proof projection-sqlite-replay-atomic --bin rpotato observability::tests::sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows
run_proof projection-lag-crash-converges --bin rpotato patch::tests::projection_lag_crash_after_lag_removal_before_journal_cleanup_converges
run_proof projection-lag-cleanup-state-closed --bin rpotato patch::tests::projection_lag_journal_cleanup_state_matrix_is_closed
run_proof projection-lag-orphan-blocks --bin rpotato patch::tests::projection_lag_orphan_without_journal_blocks
run_proof projection-lag-mutation-fails-closed --bin rpotato patch::tests::projection_lag_reference_and_member_mutation_matrix_fails_closed
run_proof projection-lag-restart-validates --bin rpotato patch::tests::projection_lag_restart_validates_reference_member_installed_bytes_and_head
run_proof projection-success-fsyncs --bin rpotato patch::tests::projection_success_receipt_requires_lag_and_journal_parent_fsyncs
run_proof projection-lag-install-failure-preserves-journal --bin rpotato patch::tests::t10_lag_install_failure_preserves_committed_journal
run_proof runtime-denial-outcomes-total --bin rpotato runtime::tests::denial_truth_table_outcome_mapping_is_total
run_proof runtime-recovery-doc-oracles --bin rpotato runtime::tests::docs_recovery_outcome_oracles_are_bilingual_and_exact
run_proof runtime-tui-outcomes-exact --bin rpotato runtime::tests::runtime_tui_outcome_oracle_all_families_exact_utf8
run_proof runtime-tui-dto-order-exact --bin rpotato runtime::tests::tui_outcome_public_dto_and_exact_fixtures_share_field_order
run_proof runtime-tui-reads-canonical --bin rpotato runtime::tests::tui_read_facade_all_views_are_canonical_bounded_fresh_and_non_mutating
run_proof filesystem-lease-single-winner --bin rpotato lease::tests::concurrent_kernel_lease_has_exactly_one_winner
run_proof ledger-event-sink-single-acquisition --bin rpotato ledger::tests::event_sink_single_acquisition_concurrency_matrix
run_proof ledger-recovery-no-nested-lease --bin rpotato ledger::tests::event_sink_crash_recovery_never_nests_ledger_lease
run_proof patch-tui-resume-revalidates --bin rpotato patch::tests::tui_workflow_resume_revalidates_lease_and_persists_exact_intent_receipt
run_proof state-bootstrap-crash-idempotent --bin rpotato state::tests::bootstrap_creation_crash_matrix_is_idempotent
run_proof state-session-new-single-commit --bin rpotato state::tests::session_new_crash_race_restart_is_single_commit
run_proof state-session-resume-ledger-first --bin rpotato state::tests::session_resume_transaction_never_exposes_current_before_ledger
run_proof state-low-level-recovery-idempotent --bin rpotato state::tests::low_level_writer_recovery_is_idempotent
run_proof state-workflow-checkpoint-crash --bin rpotato state::tests::workflow_checkpoint_writer_crash_matrix
run_proof state-workflow-recovery-prepared-only --bin rpotato state::tests::workflow_recovery_replays_only_prepared_suffix
run_proof state-active-pointer-recovery --bin rpotato state::tests::active_workflow_pointer_recovery_is_single_and_idempotent
run_proof state-terminal-pointer-cleanup --bin rpotato state::tests::terminal_pointer_cleanup_crash_race_restart_is_idempotent
run_proof state-reconcile-preserves-evidence --bin rpotato state::tests::reconcile_writer_crash_matrix_preserves_evidence
run_proof state-writer-callgraph-closed --bin rpotato state::tests::state_writer_callgraph_is_closed_and_serialized_by_project_transition
run_proof transition-projection-member-golden --bin rpotato transition::tests::projection_lag_member_full_bytes_golden_is_independent
run_proof tui-recovery-outcome-matrix --test interactive_tui interactive_tui_recovery_outcome_matrix_exact
