use crate::adapters::filesystem::{cache, layout as paths};
use crate::app::inference_adapter::{backend, model};
use crate::app::workflow_adapter::ledger;
#[cfg(test)]
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
use crate::runtime_core::reporting::runtime_report::{self, DoctorReport, InitReport};
use crate::runtime_core::workflow::application::runner::{self, RuntimeApplicationPort};
use crate::surfaces::tui::outcome::TuiOutcomeCode;
#[cfg(test)]
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, verification_credential_issued, TuiOutcomeContext,
};
use crate::{context, intent, ontology, patch, state};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

struct LegacyRuntimeApplicationPort;

impl RuntimeApplicationPort for LegacyRuntimeApplicationPort {
    fn run_agent(&mut self, request: &str) -> Result<String, AppError> {
        intent::run_report(request)
    }

    fn current_session_id(&mut self) -> Result<String, AppError> {
        Ok(ledger::validated_current_identity()?.session_id)
    }

    fn rebuild_resume_context(&mut self, session_id: &str) -> Result<String, AppError> {
        Ok(context::rebuild_resume_context(session_id, None)?.summary())
    }

    fn resume_report(&mut self) -> Result<String, AppError> {
        state::resume_report()
    }

    fn session_resume_preflight(&mut self, session_id: &str) -> Result<Option<String>, AppError> {
        state::session_resume_preflight(session_id)
    }

    fn preflight_workflow(&mut self, workflow_id: &str) -> Result<(), AppError> {
        patch::preflight_resume_workflow(workflow_id)
    }

    fn session_resume_report(&mut self, session_id: &str) -> Result<String, AppError> {
        state::session_resume_report(session_id)
    }

    fn approve_patch(
        &mut self,
        proposal_id: &str,
        token: &str,
        dry_run: bool,
        verify_command: Option<&str>,
    ) -> Result<(), AppError> {
        patch::approve_to_stdout(proposal_id, token, dry_run, verify_command)
    }

    fn verify_patch(&mut self, proposal_id: &str, token: &str) -> Result<String, AppError> {
        patch::verify_report(proposal_id, token)
    }
}

pub fn agent_run_report(request: &str) -> Result<String, AppError> {
    runner::agent_run_report(&mut LegacyRuntimeApplicationPort, request)
}

pub fn workflow_resume_report() -> Result<String, AppError> {
    runner::workflow_resume_report(&mut LegacyRuntimeApplicationPort)
}

pub fn session_resume_report(session_id: &str) -> Result<String, AppError> {
    runner::session_resume_report(&mut LegacyRuntimeApplicationPort, session_id)
}

pub fn patch_approve_to_stdout(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<(), AppError> {
    runner::patch_approve_to_stdout(
        &mut LegacyRuntimeApplicationPort,
        proposal_id,
        token,
        dry_run,
        verify_command,
    )
}

pub fn patch_verify_report(proposal_id: &str, token: &str) -> Result<String, AppError> {
    runner::patch_verify_report(&mut LegacyRuntimeApplicationPort, proposal_id, token)
}

pub fn init_report() -> Result<String, AppError> {
    let init = state::initialize()?;
    let ontology = ontology::ensure_seeded()?;
    Ok(runtime_report::render_init(InitReport {
        app_data_root: paths::app_data_root().display().to_string(),
        config_file: paths::config_file().display().to_string(),
        state_dir: paths::state_dir().display().to_string(),
        project_state_dir: paths::project_state_dir().display().to_string(),
        project_id: init.identity.project_id,
        session_id: init.identity.session_id,
        runtime_ledger: paths::runtime_ledger_file().display().to_string(),
        observability_db: init.store.path.display().to_string(),
        observability_schema: init.store.migration_version,
        ontology_store: ontology.store.display().to_string(),
        ontology_records_added: ontology.records_added,
        created_paths: init
            .created_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        backend: backend::doctor_summary(),
        model: model::candidate_summary(),
        recovered_corrupt_db: init
            .store
            .recovered_from
            .as_ref()
            .map(|path| path.display().to_string()),
    }))
}

pub fn doctor_report() -> String {
    let backend = backend::doctor_summary();
    let cache = cache::status_summary();
    let models = model::candidate_summary();
    let ontology = ontology::doctor_summary();
    let tui_outcome_codes = TuiOutcomeCode::ALL
        .iter()
        .map(|code| code.as_str().to_string())
        .collect();

    runtime_report::render_doctor(DoctorReport {
        package: env!("CARGO_PKG_NAME").to_string(),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        binary_suffix: std::env::consts::EXE_SUFFIX.to_string(),
        tui_outcome_codes,
        backend,
        model: models,
        ontology,
        cache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::tui::outcome::{TuiEffect, TuiNextAction, TuiOutcomeStatus};
    use crate::surfaces::tui::runtime_bridge::{
        OneShotSecret, TuiFreshness, TuiReadBudget, TuiReadContinuation, TuiReadRequest,
    };
    use crate::tui::canonical_read_page as read_tui_page;

    fn snapshot_tree(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
        fn visit(
            root: &std::path::Path,
            path: &std::path::Path,
            out: &mut BTreeMap<String, Vec<u8>>,
        ) {
            let entries = match std::fs::read_dir(path) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => panic!("tree snapshot read failed: {error}"),
            };
            let mut entries = entries.map(Result::unwrap).collect::<Vec<_>>();
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let relative = path.strip_prefix(root).unwrap().display().to_string();
                let metadata = std::fs::symlink_metadata(&path).unwrap();
                if metadata.file_type().is_symlink() {
                    out.insert(
                        format!("symlink:{relative}"),
                        std::fs::read_link(&path)
                            .unwrap()
                            .display()
                            .to_string()
                            .into_bytes(),
                    );
                } else if metadata.is_dir() {
                    out.insert(format!("dir:{relative}"), Vec::new());
                    visit(root, &path, out);
                } else {
                    out.insert(format!("file:{relative}"), std::fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        visit(root, root, &mut out);
        out
    }

    #[test]
    fn tui_read_budget_clamps_zero_and_overflow() {
        assert_eq!(
            TuiReadBudget::bounded(0, 0),
            TuiReadBudget {
                max_items: 1,
                max_chars: 1,
            }
        );
        assert_eq!(
            TuiReadBudget::bounded(usize::MAX, usize::MAX),
            TuiReadBudget {
                max_items: crate::surfaces::tui::runtime_bridge::TUI_MAX_ITEMS,
                max_chars: crate::surfaces::tui::runtime_bridge::TUI_MAX_CHARS,
            }
        );
    }

    #[test]
    fn approvals_never_report_complete_when_canonical_tail_is_truncated() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-runtime-approvals-truncated-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::fs::create_dir_all(paths::project_root()).unwrap();
        let initialized = state::initialize().unwrap();
        let older_approval = ledger::new_event_for(
            &initialized.identity,
            "team.admission.policy_blocked",
            "older approval",
            "bounded tail 밖의 승인",
        );
        ledger::append_event(&older_approval).unwrap();
        for index in 0..80 {
            let noise = ledger::new_event_for(
                &initialized.identity,
                "runtime.noise",
                "tail displacement",
                &format!("index={index}"),
            );
            ledger::append_event(&noise).unwrap();
        }
        state::create_workflow("refresh current-state binding").unwrap();

        let page = read_tui_page(TuiReadRequest::Approvals {
            page: 0,
            budget: TuiReadBudget::bounded(20, 24 * 1024),
        })
        .unwrap();

        assert_eq!(page.continuation, TuiReadContinuation::Truncated);
        assert!(page
            .lines
            .iter()
            .all(|line| !line.contains(&older_approval.event_id)));

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tui_read_facade_is_bounded_fresh_and_non_mutating_with_tool_output() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-runtime-read-facade-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::fs::create_dir_all(paths::project_root()).unwrap();
        state::initialize().unwrap();
        let workflow = state::create_workflow("read facade fixture").unwrap();
        let record = transcript::record_workflow_turn_with_streams(
            &workflow,
            "tool",
            "tool-read-facade",
            "tool finished",
            &[],
            Some("bounded stdout"),
            Some("bounded stderr"),
        )
        .unwrap();
        let artifact_id = record.tool_output_artifact.unwrap().id;
        let before = (
            std::fs::read(paths::current_state_file()).unwrap(),
            std::fs::read(paths::runtime_ledger_file()).unwrap(),
            std::fs::read(paths::observability_db_file()).unwrap(),
        );
        let budget = TuiReadBudget::bounded(4, 64);

        let tool = read_tui_page(TuiReadRequest::ToolOutput {
            artifact_id,
            page: 0,
            budget,
        })
        .unwrap();
        let transcript = read_tui_page(TuiReadRequest::Transcript {
            session_id: workflow.session_id.clone(),
            page: 0,
            budget,
        })
        .unwrap();
        let sessions = read_tui_page(TuiReadRequest::Sessions { page: 0, budget }).unwrap();

        assert_eq!(tool.title, "tool-output");
        assert!(tool.lines.concat().contains("artifact:"));
        assert_eq!(tool.freshness, TuiFreshness::Fresh);
        assert!(tool.lines.iter().all(|line| line.chars().count() <= 64));
        assert_eq!(transcript.freshness, TuiFreshness::Fresh);
        assert!(transcript.lines.len() <= 4);
        assert_eq!(sessions.freshness, TuiFreshness::Fresh);
        assert!(sessions.lines.len() <= 4);
        assert_eq!(
            before,
            (
                std::fs::read(paths::current_state_file()).unwrap(),
                std::fs::read(paths::runtime_ledger_file()).unwrap(),
                std::fs::read(paths::observability_db_file()).unwrap(),
            )
        );

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tui_tool_output_rejects_canonical_artifact_from_another_project() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-runtime-tool-cross-project-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = root.join("project-current");
        let data = root.join("data");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        std::fs::create_dir_all(&project).unwrap();
        state::initialize().unwrap();

        let other_identity = ledger::RuntimeIdentity {
            project_id: "project-other-security-fixture".to_string(),
            session_id: "session-other-security-fixture".to_string(),
            project_root: root.join("project-other").display().to_string(),
        };
        let other_workflow = state::WorkflowRecord::new(&other_identity, "other project tool");
        let other_record = transcript::record_workflow_turn_with_streams(
            &other_workflow,
            "tool",
            "tool-cross-project",
            "other project tool finished",
            &[],
            Some("CROSS_PROJECT_STDOUT_MUST_NOT_RENDER"),
            Some("CROSS_PROJECT_STDERR_MUST_NOT_RENDER"),
        )
        .unwrap();
        let artifact_id = other_record.tool_output_artifact.unwrap().id;

        let page = read_tui_page(TuiReadRequest::ToolOutput {
            artifact_id,
            page: 0,
            budget: TuiReadBudget::bounded(16, 64 * 1024),
        })
        .unwrap();
        assert_eq!(page.freshness, TuiFreshness::Unavailable);
        let rendered = page.lines.join("\n");
        assert!(!rendered.contains("CROSS_PROJECT_STDOUT_MUST_NOT_RENDER"));
        assert!(!rendered.contains("CROSS_PROJECT_STDERR_MUST_NOT_RENDER"));

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn tui_read_facade_all_views_are_canonical_bounded_fresh_and_non_mutating() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-runtime-read-facade-matrix-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = root.join("project");
        let data = root.join("data");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        std::fs::create_dir_all(&project).unwrap();
        state::initialize().unwrap();
        std::fs::write(project.join("fixture.txt"), "before\n").unwrap();
        let mut workflow = state::create_workflow("read facade matrix fixture").unwrap();
        let proposal = patch::prepare_workflow_proposal(
            &workflow.workflow_id,
            &workflow.action_id,
            "fixture.txt",
            "before",
            "after",
            "pwd",
        )
        .unwrap();
        let proposal_id = proposal.proposal_id.clone();
        workflow.source_path = proposal.relative_path;
        workflow.source_hash = proposal.original_sha256.clone();
        workflow.before_hash = proposal.original_sha256;
        workflow.after_hash = proposal.proposed_sha256;
        workflow.proposal_id = proposal.proposal_id;
        workflow.proposal_hash = proposal.proposal_hash;
        workflow.approval_credential_hash = proposal.approval_credential_hash;
        workflow.verification_plan = proposal.verification_command;
        workflow.approval_state = "pending".to_string();
        workflow.phase = "pending-approval".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
        let record = transcript::record_workflow_turn_with_streams(
            &workflow,
            "tool",
            "tool-read-facade-matrix",
            "canonical tool finished",
            &[],
            Some("bounded stdout"),
            Some("bounded stderr"),
        )
        .unwrap();
        let artifact_id = record.tool_output_artifact.as_ref().unwrap().id.clone();
        let existing_artifact = paths::tool_output_file(
            &workflow.project_id,
            &workflow.session_id,
            &workflow.workflow_id,
            &artifact_id,
        );
        let orphan_id = "tool-output-orphan-read-facade";
        std::fs::write(
            existing_artifact
                .parent()
                .unwrap()
                .join(format!("{orphan_id}.json")),
            std::fs::read(&existing_artifact).unwrap(),
        )
        .unwrap();
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        connection
            .execute(
                "INSERT INTO transcript_records (
                    record_id, session_id, workflow_id, ledger_event_id, event_ordinal,
                    record_kind, causal_id, content, content_hash, source_pointers_json,
                    artifact_pointer, artifact_hash, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    "record-sqlite-only-forged",
                    workflow.session_id,
                    workflow.workflow_id,
                    "event-sqlite-only-forged",
                    9_999_i64,
                    "assistant",
                    "causal-sqlite-only-forged",
                    "SQLITE_ONLY_FORGED",
                    "f".repeat(64),
                    "[]",
                    "state/transcripts/forged.json",
                    "e".repeat(64),
                    9_999_i64,
                ],
            )
            .unwrap();
        drop(connection);
        let before = (snapshot_tree(&project), snapshot_tree(&data));
        let budget = TuiReadBudget::bounded(usize::MAX, usize::MAX);

        let pages = vec![
            read_tui_page(TuiReadRequest::Overview { budget }).unwrap(),
            read_tui_page(TuiReadRequest::Monitor { budget }).unwrap(),
            read_tui_page(TuiReadRequest::Sessions { page: 0, budget }).unwrap(),
            read_tui_page(TuiReadRequest::Transcript {
                session_id: workflow.session_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::ToolOutput {
                artifact_id: artifact_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::Approvals { page: 0, budget }).unwrap(),
            read_tui_page(TuiReadRequest::Diff {
                proposal_id: proposal_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::Evidence { page: 0, budget }).unwrap(),
        ];
        let orphan = read_tui_page(TuiReadRequest::ToolOutput {
            artifact_id: orphan_id.to_string(),
            page: 0,
            budget,
        })
        .unwrap();

        assert_eq!(
            pages
                .iter()
                .map(|page| page.title.as_str())
                .collect::<Vec<_>>(),
            [
                "overview",
                "monitor",
                "sessions",
                "transcript",
                "tool-output",
                "approvals",
                "diff",
                "evidence",
            ]
        );
        for page in &pages {
            assert_eq!(page.freshness, TuiFreshness::Fresh, "{}", page.title);
            assert!(page.authority.ledger_sequence.is_some(), "{}", page.title);
            assert!(page.authority.ledger_hash.is_some(), "{}", page.title);
            assert!(page.authority.validated_at_ms.is_some(), "{}", page.title);
            assert!(page.lines.len() <= 120, "{}", page.title);
            assert!(
                page.lines
                    .iter()
                    .map(|line| line.chars().count())
                    .sum::<usize>()
                    <= 65_536,
                "{}",
                page.title
            );
        }
        assert!(!pages[3].lines.concat().contains("SQLITE_ONLY_FORGED"));
        assert_eq!(orphan.freshness, TuiFreshness::Unavailable);
        assert!(matches!(
            orphan.continuation,
            TuiReadContinuation::Unavailable | TuiReadContinuation::Truncated
        ));
        let after = (snapshot_tree(&project), snapshot_tree(&data));
        let tree_delta =
            |label: &str, before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>| {
                let mut keys = before
                    .keys()
                    .chain(after.keys())
                    .cloned()
                    .collect::<Vec<_>>();
                keys.sort();
                keys.dedup();
                keys.into_iter()
                    .filter_map(|key| {
                        let old = before.get(&key);
                        let new = after.get(&key);
                        (old != new).then(|| {
                            format!(
                                "{label}:{key}:{}->{}",
                                old.map(Vec::len)
                                    .map_or_else(|| "missing".to_string(), |len| len.to_string()),
                                new.map(Vec::len)
                                    .map_or_else(|| "missing".to_string(), |len| len.to_string())
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            };
        let mut delta = tree_delta("project", &before.0, &after.0);
        delta.extend(tree_delta("data", &before.1, &after.1));
        assert!(delta.is_empty(), "TUI read mutated state: {delta:#?}");

        let database = paths::observability_db_file();
        let hidden_database = database.with_extension("sqlite.unavailable");
        std::fs::rename(&database, &hidden_database).unwrap();
        let unavailable_before = (snapshot_tree(&project), snapshot_tree(&data));
        let unavailable_pages = vec![
            read_tui_page(TuiReadRequest::Overview { budget }).unwrap(),
            read_tui_page(TuiReadRequest::Monitor { budget }).unwrap(),
            read_tui_page(TuiReadRequest::Sessions {
                page: u64::MAX,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::Transcript {
                session_id: workflow.session_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::ToolOutput {
                artifact_id: artifact_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::Approvals { page: 0, budget }).unwrap(),
            read_tui_page(TuiReadRequest::Diff {
                proposal_id: proposal_id.clone(),
                page: 0,
                budget,
            })
            .unwrap(),
            read_tui_page(TuiReadRequest::Evidence { page: 0, budget }).unwrap(),
        ];
        for page in &unavailable_pages {
            assert_eq!(page.freshness, TuiFreshness::Unavailable, "{}", page.title);
            assert_eq!(page.authority.projected_sequence, None, "{}", page.title);
        }
        assert!(unavailable_pages[2].lines.is_empty());
        assert_eq!(
            unavailable_before,
            (snapshot_tree(&project), snapshot_tree(&data)),
            "unavailable projection reads must not mutate any file"
        );
        std::fs::rename(&hidden_database, &database).unwrap();

        let connection = rusqlite::Connection::open(&database).unwrap();
        assert_eq!(
            connection
                .execute(
                    "DELETE FROM ledger_events WHERE rowid = (SELECT MAX(rowid) FROM ledger_events)",
                    [],
                )
                .unwrap(),
            1
        );
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        drop(connection);
        let stale_before = (snapshot_tree(&project), snapshot_tree(&data));
        let stale = read_tui_page(TuiReadRequest::Overview { budget }).unwrap();
        assert_eq!(stale.freshness, TuiFreshness::Stale);
        assert_eq!(
            stale.authority.projected_sequence,
            stale
                .authority
                .ledger_sequence
                .and_then(|sequence| sequence.checked_sub(1))
        );
        assert_eq!(
            stale_before,
            (snapshot_tree(&project), snapshot_tree(&data)),
            "stale projection read must not mutate DB/WAL/SHM or canonical state"
        );

        std::fs::create_dir_all(paths::projection_lag_dir()).unwrap();
        std::fs::write(
            paths::projection_lag_dir().join("corrupt-unbound.json"),
            "{}",
        )
        .unwrap();
        let corrupt_before = (snapshot_tree(&project), snapshot_tree(&data));
        let corrupt = read_tui_page(TuiReadRequest::Overview { budget }).unwrap();
        assert_eq!(corrupt.freshness, TuiFreshness::Unavailable);
        assert_eq!(
            corrupt_before,
            (snapshot_tree(&project), snapshot_tree(&data)),
            "corrupt projection-lag candidate must fail closed without mutation"
        );

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn denial_truth_table_outcome_mapping_is_total() {
        let codes = [
            TuiOutcomeCode::DenyPatchAccepted,
            TuiOutcomeCode::DenyVerificationRolledBack,
            TuiOutcomeCode::DenyBlockedNotPending,
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeCode::RollbackConflict,
            TuiOutcomeCode::CancelAccepted,
            TuiOutcomeCode::CancelPhaseBlocked,
            TuiOutcomeCode::CancelTerminalBlocked,
            TuiOutcomeCode::CancelNoActiveWorkflow,
            TuiOutcomeCode::ResumeAccepted,
            TuiOutcomeCode::ResumeStaleSelection,
            TuiOutcomeCode::ResumeCorruptState,
            TuiOutcomeCode::ResumeInconclusiveEffect,
            TuiOutcomeCode::SecretRefreshOnly,
            TuiOutcomeCode::TerminalCapabilitySizeRead,
            TuiOutcomeCode::TerminalCapabilityModeRead,
            TuiOutcomeCode::TerminalNoEchoSetFailed,
            TuiOutcomeCode::TerminalSecretReadFailed,
            TuiOutcomeCode::TerminalFrameWritePreDispatch,
            TuiOutcomeCode::TerminalFrameWritePostDispatch,
            TuiOutcomeCode::SourceInstallRecoveryRequired,
            TuiOutcomeCode::SourceInstallRecoveryConflict,
            TuiOutcomeCode::SourceInstallRecoveryComplete,
            TuiOutcomeCode::ProjectionRepairRequired,
            TuiOutcomeCode::ProjectionLagInstallFailed,
            TuiOutcomeCode::ProjectionRepairComplete,
            TuiOutcomeCode::SourceInstallUnsupportedPlatform,
        ];
        let unique = codes
            .iter()
            .map(|code| code.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(codes.len(), 27);
        assert_eq!(unique.len(), 27);
        for (phase, code) in [
            ("pending-approval", TuiOutcomeCode::DenyPatchAccepted),
            (
                "pending-verification-approval",
                TuiOutcomeCode::DenyVerificationRolledBack,
            ),
            ("approved", TuiOutcomeCode::DenyBlockedNotPending),
            (
                "verification-approved",
                TuiOutcomeCode::DenyBlockedNotPending,
            ),
            (
                "verification-started",
                TuiOutcomeCode::DenyBlockedNotPending,
            ),
            ("verified", TuiOutcomeCode::DenyBlockedNotPending),
            ("complete", TuiOutcomeCode::DenyBlockedTerminalState),
            ("failed", TuiOutcomeCode::DenyBlockedTerminalState),
            ("cancelled", TuiOutcomeCode::DenyBlockedTerminalState),
        ] {
            assert_eq!(
                patch::denial_phase_outcome_code(phase),
                Some(code),
                "production denial dispatch mismatch for phase: {phase}"
            );
        }
        assert_eq!(patch::denial_phase_outcome_code("unknown"), None);
    }

    #[test]
    fn runtime_tui_outcome_oracle_all_families_exact_utf8() {
        let intent = "intent-outcome-0001";
        let workflow = "workflow-outcome-0001";
        let context = |phase| TuiOutcomeContext {
            intent_id: Some(intent),
            workflow_id: Some(workflow),
            phase: Some(phase),
            platform: Some("windows"),
        };
        let fixtures = [
            (
                TuiOutcomeCode::DenyPatchAccepted,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::InspectDeniedReceipt,
                "패치 적용 거부 완료\n- code: deny.patch.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 소스 변경 없이 취소 상태를 기록했습니다.\n- 다음: 거부 영수증을 확인하세요.",
            ),
            (
                TuiOutcomeCode::DenyVerificationRolledBack,
                context("pending-verification-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::RolledBack,
                TuiFreshness::Fresh,
                TuiNextAction::InspectRollbackReceipt,
                "검증 거부 및 롤백 완료\n- code: deny.verification.rolled-back\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 원본 해시를 검증하고 취소 상태를 기록했습니다.\n- 다음: 롤백 영수증을 확인하세요.",
            ),
            (
                TuiOutcomeCode::DenyBlockedNotPending,
                context("verification-started"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::UseCancelOrRefresh,
                "승인 대기 상태가 아니어서 거부 차단\n- code: deny.blocked.not-pending\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 승인 상태와 효과를 변경하지 않았습니다.\n- 다음: 취소를 사용하거나 정본 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::DenyBlockedTerminalState,
                context("complete"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::InspectTerminalReceipt,
                "종료 상태여서 거부 차단\n- code: deny.blocked.terminal-state\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- phase: complete\n- 동작: 종료 상태와 영수증을 변경하지 않았습니다.\n- 다음: 기존 종료 영수증을 확인하세요.",
            ),
            (
                TuiOutcomeCode::RollbackConflict,
                context("pending-verification-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::ResolveRollbackConflict,
                "롤백 충돌로 차단됨\n- code: rollback.conflict\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 현재 포인터와 소스는 변경하지 않았습니다.\n- 다음: 소스 충돌을 해결한 뒤 다시 읽으세요.",
            ),
            (
                TuiOutcomeCode::CancelAccepted,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshCanonicalState,
                "워크플로 취소 완료\n- code: cancel.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 취소 상태를 기록했습니다.\n- 다음: 정본 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::CancelPhaseBlocked,
                context("verification-started"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::ChooseCancellablePhase,
                "현재 단계에서는 취소할 수 없음\n- code: cancel.phase-blocked\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 취소 가능한 단계를 확인하세요.",
            ),
            (
                TuiOutcomeCode::CancelTerminalBlocked,
                context("complete"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::CloseOrInspectTerminal,
                "종료된 워크플로는 취소할 수 없음\n- code: cancel.terminal-blocked\n- workflow: workflow-outcome-0001\n- phase: complete\n- 동작: 종료 상태를 유지했습니다.\n- 다음: 종료 영수증을 확인하세요.",
            ),
            (
                TuiOutcomeCode::CancelNoActiveWorkflow,
                context("complete"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::SelectActiveWorkflow,
                "취소할 활성 워크플로가 없음\n- code: cancel.no-active-workflow\n- 동작: 상태를 변경하지 않았습니다.\n- 다음: 활성 워크플로를 선택하세요.",
            ),
            (
                TuiOutcomeCode::ResumeAccepted,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshCanonicalState,
                "워크플로 재개 완료\n- code: resume.accepted\n- intent: intent-outcome-0001\n- workflow: workflow-outcome-0001\n- 동작: 검증된 정본 상태에서 재개했습니다.\n- 다음: 정본 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::ResumeStaleSelection,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::RetryResumeAfterRefresh,
                "오래된 선택으로 재개 차단\n- code: resume.stale-selection\n- workflow: workflow-outcome-0001\n- 동작: 상태를 변경하거나 효과를 재실행하지 않았습니다.\n- 다음: 새로고침 후 다시 선택하세요.",
            ),
            (
                TuiOutcomeCode::ResumeCorruptState,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::RepairCorruptState,
                "손상된 상태로 재개 차단\n- code: resume.corrupt-state\n- workflow: workflow-outcome-0001\n- 동작: 상태와 효과를 변경하지 않았습니다.\n- 다음: 정본 상태와 해시를 복구하세요.",
            ),
            (
                TuiOutcomeCode::ResumeInconclusiveEffect,
                context("verification-started"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Stale,
                TuiNextAction::ResolveInconclusiveEffect,
                "불확실한 효과로 자동 재개 차단\n- code: resume.inconclusive-effect\n- workflow: workflow-outcome-0001\n- phase: verification-started\n- 동작: 모델 또는 검증 명령을 재실행하지 않았습니다.\n- 다음: 효과를 확인하고 명시적으로 정리하세요.",
            ),
            (
                TuiOutcomeCode::SecretRefreshOnly,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Committed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshOnly,
                "커밋 완료, 비밀값 재표시 불가\n- code: secret.refresh-only\n- intent: intent-outcome-0001\n- 동작: 커밋 영수증만 반환합니다.\n- 다음: 비밀값 없이 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::TerminalCapabilitySizeRead,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::ReadOnly,
                "터미널 크기 확인 실패\n- code: terminal.capability.size-read\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 읽기 전용 모드를 사용하세요.",
            ),
            (
                TuiOutcomeCode::TerminalCapabilityModeRead,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::ReadOnly,
                "터미널 모드 확인 실패\n- code: terminal.capability.mode-read\n- 동작: 모드와 상태를 변경하지 않았습니다.\n- 다음: 터미널 모드를 확인한 뒤 다시 시도하세요.",
            ),
            (
                TuiOutcomeCode::TerminalNoEchoSetFailed,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::ReadOnly,
                "비밀 입력 보호 설정 실패\n- code: terminal.no-echo-set.failed\n- 동작: 비밀값을 읽거나 요청을 보내지 않았습니다.\n- 다음: 무반향 입력을 복구하세요.",
            ),
            (
                TuiOutcomeCode::TerminalSecretReadFailed,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Unavailable,
                TuiNextAction::RetryInput,
                "비밀 입력 읽기 실패\n- code: terminal.secret-read.failed\n- 동작: 비밀값을 수락하거나 저장하지 않았습니다.\n- 다음: 새 입력으로 다시 시도하세요.",
            ),
            (
                TuiOutcomeCode::TerminalFrameWritePreDispatch,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Stale,
                TuiNextAction::RetryIntent,
                "요청 전 화면 출력 실패\n- code: terminal.frame-write.pre-dispatch\n- intent: intent-outcome-0001\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 정본 상태를 다시 읽고 요청하세요.",
            ),
            (
                TuiOutcomeCode::TerminalFrameWritePostDispatch,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::Committed,
                TuiFreshness::Stale,
                TuiNextAction::RefreshOnly,
                "커밋 후 화면 출력 실패\n- code: terminal.frame-write.post-dispatch\n- intent: intent-outcome-0001\n- 동작: 요청을 다시 보내지 않습니다.\n- 다음: 영수증을 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::SourceInstallRecoveryRequired,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Stale,
                TuiNextAction::RepairSourceInstall,
                "소스 설치 복구 필요\n- code: source-install.recovery-required\n- intent: intent-outcome-0001\n- 동작: 저널과 복구 증거를 보존했습니다.\n- 다음: 동일 저널로 복구를 실행하세요.",
            ),
            (
                TuiOutcomeCode::SourceInstallRecoveryConflict,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::Unavailable,
                TuiNextAction::ResolveSourceConflict,
                "소스 설치 복구 충돌\n- code: source-install.recovery-conflict\n- intent: intent-outcome-0001\n- 동작: 대상과 저널을 덮어쓰지 않았습니다.\n- 다음: 경로와 해시 충돌을 해결하세요.",
            ),
            (
                TuiOutcomeCode::SourceInstallRecoveryComplete,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Refreshed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshSourceState,
                "소스 설치 복구 완료\n- code: source-install.recovery-complete\n- intent: intent-outcome-0001\n- 동작: 준비된 바이트로 정확히 수렴했습니다.\n- 다음: 소스 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::ProjectionRepairRequired,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::ProjectionLag,
                TuiNextAction::RepairProjection,
                "파생 출력 복구 필요\n- code: projection.repair-required\n- intent: intent-outcome-0001\n- 동작: 저널과 지연 표식을 보존했습니다.\n- 다음: project ledger, operation log, SQLite 순서로 복구하세요.",
            ),
            (
                TuiOutcomeCode::ProjectionLagInstallFailed,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::RecoveryPending,
                TuiFreshness::ProjectionLag,
                TuiNextAction::RepairProjection,
                "지연 표식 설치 실패\n- code: projection.lag-install-failed\n- intent: intent-outcome-0001\n- 동작: 저널을 보존하고 정리를 중단했습니다.\n- 다음: 지연 표식을 다시 설치한 뒤 파생 출력을 복구하세요.",
            ),
            (
                TuiOutcomeCode::ProjectionRepairComplete,
                context("pending-approval"),
                TuiOutcomeStatus::Succeeded,
                TuiEffect::Refreshed,
                TuiFreshness::Fresh,
                TuiNextAction::RefreshProjection,
                "파생 출력 복구 완료\n- code: projection.repair-complete\n- intent: intent-outcome-0001\n- 동작: 지연 표식과 저널 정리를 완료했습니다.\n- 다음: 투영 상태를 새로고침하세요.",
            ),
            (
                TuiOutcomeCode::SourceInstallUnsupportedPlatform,
                context("pending-approval"),
                TuiOutcomeStatus::Blocked,
                TuiEffect::NotDispatched,
                TuiFreshness::Fresh,
                TuiNextAction::UseUnixOrChooseNonSourceAction,
                "source install 차단\n- code: source-install.unsupported-platform\n- platform: windows\n- 지원 범위: v0.34.0 source installation은 Unix만 지원합니다.\n- 동작: journal/temp/guard/rollback/target 변경 없음",
            ),
        ];

        assert_eq!(fixtures.len(), 27);
        for (code, context, status, effect, freshness, next_action, message) in fixtures {
            let outcome = exact_tui_outcome(code, context).unwrap();
            assert_eq!(outcome.status, status, "{} status", code.as_str());
            assert_eq!(outcome.code, code, "{} code", code.as_str());
            assert_eq!(outcome.effect, effect, "{} effect", code.as_str());
            assert_eq!(outcome.safe_message.as_bytes(), message.as_bytes());
            assert_eq!(outcome.freshness, freshness, "{} freshness", code.as_str());
            assert_eq!(outcome.next_action, next_action, "{} action", code.as_str());
            assert!(
                outcome.one_shot_secret.is_none(),
                "{} secret",
                code.as_str()
            );
        }
    }

    #[test]
    fn source_install_unsupported_platform_result_is_exact() {
        let outcome =
            crate::surfaces::tui::outcome::unsupported_source_platform_outcome("windows").unwrap();

        assert_eq!(outcome.status, TuiOutcomeStatus::Blocked);
        assert_eq!(
            outcome.code,
            TuiOutcomeCode::SourceInstallUnsupportedPlatform
        );
        assert_eq!(outcome.effect, TuiEffect::NotDispatched);
        assert_eq!(
            outcome.safe_message.as_bytes(),
            b"source install \xec\xb0\xa8\xeb\x8b\xa8\n- code: source-install.unsupported-platform\n- platform: windows\n- \xec\xa7\x80\xec\x9b\x90 \xeb\xb2\x94\xec\x9c\x84: v0.34.0 source installation\xec\x9d\x80 Unix\xeb\xa7\x8c \xec\xa7\x80\xec\x9b\x90\xed\x95\xa9\xeb\x8b\x88\xeb\x8b\xa4.\n- \xeb\x8f\x99\xec\x9e\x91: journal/temp/guard/rollback/target \xeb\xb3\x80\xea\xb2\xbd \xec\x97\x86\xec\x9d\x8c"
        );
        assert_eq!(outcome.freshness, TuiFreshness::Fresh);
        assert_eq!(
            outcome.next_action,
            TuiNextAction::UseUnixOrChooseNonSourceAction
        );
        assert!(outcome.one_shot_secret.is_none());
    }

    #[test]
    fn tui_outcome_public_dto_and_exact_fixtures_share_field_order() {
        let source = include_str!("surfaces/tui/outcome.rs");
        let start = source.find("pub(crate) struct TuiOutcome {").unwrap();
        let end = source[start..].find("\n}").unwrap() + start;
        let definition = &source[start..end];
        let fields = [
            "pub(crate) status:",
            "pub(crate) code:",
            "pub(crate) effect:",
            "pub(crate) safe_message:",
            "pub(crate) freshness:",
            "pub(crate) next_action:",
            "pub(crate) one_shot_secret:",
        ];
        let positions = fields
            .iter()
            .map(|field| definition.find(field).unwrap())
            .collect::<Vec<_>>();

        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(definition.matches("    pub(crate) ").count(), fields.len());
    }

    #[test]
    fn one_shot_secret_plaintext_accessor_consumes_value() {
        assert!(include_str!("surfaces/tui/runtime_bridge.rs")
            .contains("fn expose<R>(self, use_plaintext: impl FnOnce(&str) -> R) -> R"));
        let secret = OneShotSecret::new("secret-value".to_string()).unwrap();
        assert_eq!(secret.expose(str::to_string), "secret-value");
        assert!(OneShotSecret::new(String::new()).is_err());
    }

    #[test]
    fn immediate_credential_outcome_is_separate_from_the_27_exact_rows() {
        let credential = "ab".repeat(32);
        let outcome = verification_credential_issued(
            "intent-credential-issued",
            OneShotSecret::new(credential.clone()).unwrap(),
        )
        .unwrap();

        assert_eq!(TuiOutcomeCode::ALL.len(), 27);
        assert!(!TuiOutcomeCode::ALL.contains(&TuiOutcomeCode::VerificationCredentialIssued));
        assert_eq!(outcome.code, TuiOutcomeCode::VerificationCredentialIssued);
        assert!(!outcome.safe_message.contains(&credential));
        assert_eq!(
            outcome.one_shot_secret.unwrap().expose(str::to_string),
            credential
        );
        assert!(exact_tui_outcome(
            TuiOutcomeCode::VerificationCredentialIssued,
            TuiOutcomeContext::default()
        )
        .is_err());
    }

    #[test]
    fn docs_recovery_outcome_oracles_are_bilingual_and_exact() {
        let english = include_str!("../docs/tui.md");
        let korean = include_str!("../docs/ko/tui.md");
        let contract = |document: &str| {
            document
                .split_once("<!-- TUI-READ-CONTRACT:START -->\n")
                .and_then(|(_, tail)| tail.split_once("\n<!-- TUI-READ-CONTRACT:END -->"))
                .map(|(body, _)| body.to_string())
                .expect("exact TUI read contract markers")
        };
        assert_eq!(
            contract(english),
            "The eight views (`overview`, `monitor`, `sessions`, `transcript`, `tool-output`,\n`approvals`, `diff`, and `evidence`) use view-specific item, byte, scan, line, and\npagination bounds. Every page carries canonical current/workflow revision and hash,\nledger sequence and hash, relevant content or transcript hash, projection watermark,\nvalidation time, and one typed continuation: `complete`, `next-page`, `truncated`,\n`unavailable`, or `redacted`. SQLite is a derived metrics/freshness projection only;\nfreshness is exactly `fresh`, `stale`, `projection-lag`, or `unavailable`. Read paths do\nnot acquire mutation leases, repair state, write validation gaps, or admit corrupt,\nunbound, SQLite-only, or directory-scan-only candidates."
        );
        assert_eq!(
            contract(korean),
            "8개 view(`overview`, `monitor`, `sessions`, `transcript`, `tool-output`, `approvals`,\n`diff`, `evidence`)는 view별 item, byte, scan, line, pagination 상한을 적용합니다. 모든\npage는 canonical current/workflow revision과 hash, ledger sequence와 hash, 관련 content\n또는 transcript hash, projection watermark, validation time, 그리고 `complete`,\n`next-page`, `truncated`, `unavailable`, `redacted` 중 하나의 typed continuation을\n포함합니다. SQLite는 파생된 metrics/freshness projection일 뿐이며 freshness 표기는 정확히\n`fresh`, `stale`, `projection-lag`, `unavailable`입니다. 읽기 경로는 mutation lease를\n획득하거나 state를 복구하거나 validation gap을 쓰지 않으며 corrupt, unbound,\nSQLite-only, directory-scan-only candidate를 허용하지 않습니다."
        );
        assert!(english.contains("closed 27-row\noutcome table"));
        assert!(english.contains("exact E9 lag marker until repair converges"));
        assert!(korean.contains("closed 27-row outcome table"));
        assert!(korean.contains("exact E9 lag marker를 보존"));
    }

    #[test]
    fn patch_terminal_guard_is_scoped_to_completion_reports() {
        let terminal = "패치 작업 완료\nSummary\n- 결과: 성공".to_string();
        assert_eq!(
            runtime_report::guard_patch_terminal(terminal.clone()),
            crate::runtime_core::reporting::korean_guard::guard_or_failure(&terminal)
        );

        let non_terminal = "patch approve\nSummary\n- status: applied".to_string();
        assert_eq!(
            runtime_report::guard_patch_terminal(non_terminal.clone()),
            non_terminal
        );
    }

    #[test]
    fn doctor_report_field_order_is_stable() {
        let prefixes = doctor_report()
            .lines()
            .map(|line| {
                line.split_once(':')
                    .map_or(line, |(prefix, _)| prefix)
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            prefixes,
            [
                "rpotato 진단",
                "- CLI",
                "- package",
                "- package version",
                "- release target os",
                "- release target arch",
                "- release binary suffix",
                "- release smoke",
                "- TUI outcome contract",
                "- runtime core",
                "- backend",
                "- model",
                "- ontology",
                "- cache",
            ]
        );
    }

    #[test]
    fn doctor_report_includes_release_smoke_fields() {
        let report = doctor_report();

        assert!(report.contains("package: rpotato"));
        assert!(report.contains(&format!("package version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains("release target os:"));
        assert!(report.contains("release target arch:"));
        assert!(report.contains("release binary suffix:"));
        assert!(report.contains("release smoke: available"));
    }
}
