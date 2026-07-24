//! Surface-neutral runtime report inputs and canonical rendering.

use super::korean_guard;

const RELEASE_SMOKE_SUMMARY: &str = "available; doctor does not download models, install backends, start sidecars, or require network access";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowResumeReport {
    pub continuation: String,
    pub reconstructed_context: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionResumeReport {
    pub selection: String,
    pub reconstructed_context: String,
    pub continuation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InitReport {
    pub app_data_root: String,
    pub config_file: String,
    pub state_dir: String,
    pub project_state_dir: String,
    pub project_id: String,
    pub session_id: String,
    pub runtime_ledger: String,
    pub observability_db: String,
    pub observability_schema: i64,
    pub ontology_store: String,
    pub ontology_records_added: usize,
    pub created_paths: Vec<String>,
    pub backend: String,
    pub model: String,
    pub recovered_corrupt_db: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorReport {
    pub package: String,
    pub package_version: String,
    pub target_os: String,
    pub target_arch: String,
    pub binary_suffix: String,
    pub tui_outcome_codes: Vec<String>,
    pub backend: String,
    pub model: String,
    pub web_search: String,
    pub ontology: String,
    pub cache: String,
}

pub(crate) fn render_workflow_resume(report: WorkflowResumeReport) -> String {
    format!(
        "{}\n- reconstructed context: {}",
        guard_patch_terminal(report.continuation),
        report.reconstructed_context
    )
}

pub(crate) fn render_session_resume(report: SessionResumeReport) -> String {
    format!(
        "{}\n- reconstructed context: {}\n- continuation:\n{}",
        report.selection,
        report.reconstructed_context,
        guard_patch_terminal(report.continuation)
    )
}

pub(crate) fn guard_patch_terminal(report: String) -> String {
    if report.starts_with("패치 작업 ") {
        korean_guard::guard_or_failure(&report)
    } else {
        report
    }
}

pub(crate) fn render_init(report: InitReport) -> String {
    let created = if report.created_paths.is_empty() {
        "새로 만든 디렉터리 없음".to_string()
    } else {
        report
            .created_paths
            .iter()
            .map(|path| format!("  - {path}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let recovered = report
        .recovered_corrupt_db
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {path}"))
        .unwrap_or_default();

    format!(
        "rpotato init 결과\n- app data root: {}\n- config file: {}\n- state dir: {}\n- project state dir: {}\n- project id: {}\n- session id: {}\n- runtime ledger: {}\n- observability db: {} (schema v{})\n- ontology store: {} (added Layer A records {})\n- created paths:\n{}\n- backend: {}\n- model: {}\n- 동작: 상태 디렉터리, current-state, ledger, SQLite projection, ontology store/schema를 초기화했습니다. 모델/backend 다운로드는 수행하지 않았습니다.{}",
        report.app_data_root,
        report.config_file,
        report.state_dir,
        report.project_state_dir,
        report.project_id,
        report.session_id,
        report.runtime_ledger,
        report.observability_db,
        report.observability_schema,
        report.ontology_store,
        report.ontology_records_added,
        created,
        report.backend,
        report.model,
        recovered
    )
}

pub(crate) fn render_doctor(report: DoctorReport) -> String {
    let tui_outcome_contract = report.tui_outcome_codes.join(",");

    format!(
        "rpotato 진단\n- CLI: 사용 가능\n- package: {}\n- package version: {}\n- release target os: {}\n- release target arch: {}\n- release binary suffix: {}\n- release smoke: {}\n- TUI outcome contract: {} codes ({})\n- runtime core: durable workflow/report boundary 사용\n- backend: {}\n- model: {}\n- web search: {}\n- ontology: {}\n- cache: {}",
        report.package,
        report.package_version,
        report.target_os,
        report.target_arch,
        report.binary_suffix,
        RELEASE_SMOKE_SUMMARY,
        report.tui_outcome_codes.len(),
        tui_outcome_contract,
        report.backend,
        report.model,
        report.web_search,
        report.ontology,
        report.cache
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_report_bytes_and_guard_scope_are_stable() {
        assert_eq!(
            render_workflow_resume(WorkflowResumeReport {
                continuation: "patch approve\n- status: pending".into(),
                reconstructed_context: "turns=2".into(),
            }),
            "patch approve\n- status: pending\n- reconstructed context: turns=2"
        );
        assert_eq!(
            render_session_resume(SessionResumeReport {
                selection: "session resume\n- session id: session-1".into(),
                reconstructed_context: "turns=3".into(),
                continuation: "패치 작업 완료\nSummary\n- 결과: 성공".into(),
            }),
            "session resume\n- session id: session-1\n- reconstructed context: turns=3\n- continuation:\n패치 작업 완료\n- 결과: 성공"
        );
    }

    #[test]
    fn init_report_field_order_and_recovery_suffix_are_stable() {
        let rendered = render_init(InitReport {
            app_data_root: "/data".into(),
            config_file: "/data/config.toml".into(),
            state_dir: "/data/state".into(),
            project_state_dir: "/project/.rpotato".into(),
            project_id: "project-1".into(),
            session_id: "session-1".into(),
            runtime_ledger: "/data/state/runtime-ledger.jsonl".into(),
            observability_db: "/data/state/observability.sqlite3".into(),
            observability_schema: 7,
            ontology_store: "/data/state/ontology.json".into(),
            ontology_records_added: 3,
            created_paths: Vec::new(),
            backend: "backend-ready".into(),
            model: "model-ready".into(),
            recovered_corrupt_db: Some("/data/state/corrupt.sqlite3".into()),
        });

        assert_eq!(
            rendered,
            "rpotato init 결과\n- app data root: /data\n- config file: /data/config.toml\n- state dir: /data/state\n- project state dir: /project/.rpotato\n- project id: project-1\n- session id: session-1\n- runtime ledger: /data/state/runtime-ledger.jsonl\n- observability db: /data/state/observability.sqlite3 (schema v7)\n- ontology store: /data/state/ontology.json (added Layer A records 3)\n- created paths:\n새로 만든 디렉터리 없음\n- backend: backend-ready\n- model: model-ready\n- 동작: 상태 디렉터리, current-state, ledger, SQLite projection, ontology store/schema를 초기화했습니다. 모델/backend 다운로드는 수행하지 않았습니다.\n- recovered corrupt db: /data/state/corrupt.sqlite3"
        );
    }

    #[test]
    fn doctor_report_bytes_are_stable() {
        assert_eq!(
            render_doctor(DoctorReport {
                package: "rpotato".into(),
                package_version: "0.37.0".into(),
                target_os: "linux".into(),
                target_arch: "x86_64".into(),
                binary_suffix: "".into(),
                tui_outcome_codes: vec!["first".into(), "second".into()],
                backend: "backend-ready".into(),
                model: "model-ready".into(),
                web_search: "search-ready".into(),
                ontology: "ontology-ready".into(),
                cache: "cache-ready".into(),
            }),
            "rpotato 진단\n- CLI: 사용 가능\n- package: rpotato\n- package version: 0.37.0\n- release target os: linux\n- release target arch: x86_64\n- release binary suffix: \n- release smoke: available; doctor does not download models, install backends, start sidecars, or require network access\n- TUI outcome contract: 2 codes (first,second)\n- runtime core: durable workflow/report boundary 사용\n- backend: backend-ready\n- model: model-ready\n- web search: search-ready\n- ontology: ontology-ready\n- cache: cache-ready"
        );
    }
}
