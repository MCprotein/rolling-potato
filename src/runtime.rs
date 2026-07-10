use crate::{backend, cache, intent, model, ontology, patch, paths, state};

pub fn agent_run_report(request: &str) -> Result<String, crate::app::AppError> {
    intent::run_report(request)
}

pub fn workflow_resume_report() -> Result<String, crate::app::AppError> {
    let report = state::resume_report()?;
    Ok(guard_patch_terminal_report(report))
}

pub fn patch_approve_report(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<String, crate::app::AppError> {
    let report = patch::approve_report(proposal_id, token, dry_run, verify_command)?;
    Ok(guard_patch_terminal_report(report))
}

fn guard_patch_terminal_report(report: String) -> String {
    if report.starts_with("패치 작업 ") {
        crate::korean_guard::guard_or_failure(&report)
    } else {
        report
    }
}

pub fn init_report() -> Result<String, crate::app::AppError> {
    let init = state::initialize()?;
    let ontology = ontology::ensure_seeded()?;
    let created = if init.created_paths.is_empty() {
        "새로 만든 디렉터리 없음".to_string()
    } else {
        init.created_paths
            .iter()
            .map(|path| format!("  - {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let recovered = init
        .store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "rpotato init 결과\n- app data root: {}\n- config file: {}\n- state dir: {}\n- project state dir: {}\n- project id: {}\n- session id: {}\n- runtime ledger: {}\n- observability db: {} (schema v{})\n- ontology store: {} (added Layer A records {})\n- created paths:\n{}\n- backend: {}\n- model: {}\n- 동작: 상태 디렉터리, current-state, ledger, SQLite projection, ontology store/schema를 초기화했습니다. 모델/backend 다운로드는 수행하지 않았습니다.{}",
        paths::app_data_root().display(),
        paths::config_file().display(),
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        init.identity.project_id,
        init.identity.session_id,
        paths::runtime_ledger_file().display(),
        init.store.path.display(),
        init.store.migration_version,
        ontology.store.display(),
        ontology.records_added,
        created,
        backend::doctor_summary(),
        model::candidate_summary(),
        recovered
    ))
}

pub fn doctor_report() -> String {
    let backend = backend::doctor_summary();
    let cache = cache::status_summary();
    let models = model::candidate_summary();
    let ontology = ontology::doctor_summary();
    let release = release_smoke_summary();

    format!(
        "rpotato 진단\n- CLI: 사용 가능\n- package: {}\n- package version: {}\n- release target os: {}\n- release target arch: {}\n- release binary suffix: {}\n- release smoke: {}\n- runtime core: durable workflow/report boundary 사용\n- backend: {}\n- model: {}\n- ontology: {}\n- cache: {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::EXE_SUFFIX,
        release,
        backend,
        models,
        ontology,
        cache
    )
}

fn release_smoke_summary() -> &'static str {
    "available; doctor does not download models, install backends, start sidecars, or require network access"
}

#[cfg(test)]
mod tests {
    use super::*;

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
