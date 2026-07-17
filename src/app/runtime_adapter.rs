//! Runtime application adapters and top-level operational reports.

use crate::adapters::filesystem::{cache, layout as paths};
use crate::app::context_adapter as context;
use crate::app::inference_adapter::{backend, model};
use crate::app::intent_adapter as intent;
use crate::app::ontology_adapter as ontology;
use crate::app::patch_adapter as patch;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::reporting::runtime_report::{self, DoctorReport, InitReport};
use crate::runtime_core::workflow::application::runner::{self, RuntimeApplicationPort};
use crate::surfaces::tui::outcome::TuiOutcomeCode;

struct RuntimeApplicationAdapter;

impl RuntimeApplicationPort for RuntimeApplicationAdapter {
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
    runner::agent_run_report(&mut RuntimeApplicationAdapter, request)
}

pub fn workflow_resume_report() -> Result<String, AppError> {
    runner::workflow_resume_report(&mut RuntimeApplicationAdapter)
}

pub fn session_resume_report(session_id: &str) -> Result<String, AppError> {
    runner::session_resume_report(&mut RuntimeApplicationAdapter, session_id)
}

pub fn patch_approve_to_stdout(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<(), AppError> {
    runner::patch_approve_to_stdout(
        &mut RuntimeApplicationAdapter,
        proposal_id,
        token,
        dry_run,
        verify_command,
    )
}

pub fn patch_verify_report(proposal_id: &str, token: &str) -> Result<String, AppError> {
    runner::patch_verify_report(&mut RuntimeApplicationAdapter, proposal_id, token)
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
#[path = "runtime_adapter/tests.rs"]
mod tests;
