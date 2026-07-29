use super::*;

#[path = "tests/admission.rs"]
mod admission;
#[path = "tests/contract.rs"]
mod contract;
#[path = "tests/execution.rs"]
mod execution;
#[path = "tests/persistence.rs"]
mod persistence;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn launch(role: &str) -> ValidatedLaunch {
    let tools = if role == "executor" {
        strings(&["read_file", "render_diff"])
    } else {
        strings(&["read_file"])
    };
    let writes = if role == "executor" {
        strings(&["src/subagent.rs"])
    } else {
        Vec::new()
    };
    validate_launch(
        role,
        "bounded task",
        &tools,
        &strings(&["src/main.rs"]),
        &writes,
        None,
        None,
    )
    .unwrap()
}

fn record(role: &str) -> SubagentRecordV1 {
    SubagentRecordV1::new(
        "project-test",
        "session-test",
        "workflow-test",
        1,
        &"a".repeat(64),
        launch(role),
    )
    .unwrap()
}

fn initialize_parent() -> state::WorkflowRecord {
    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::initialize().unwrap();
    state::create_workflow("subagent parent fixture").unwrap()
}

fn completed_result(
    record: &SubagentRecordV1,
    context: &crate::app::context_adapter::ContextPack,
) -> String {
    let evidence_ref = &context.source_pointers[0].stable_ref;
    format!(
        "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"검증된 결과\",\"findings\":[\"선언된 파일을 확인했습니다.\"],\"patch_proposal\":null,\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모 작업을 계속합니다.\"}}",
        record.subagent_id,
        record.parent_workflow_id,
        record.role.as_str(),
        evidence_ref,
    )
}
