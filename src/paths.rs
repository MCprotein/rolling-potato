use std::env;
use std::path::PathBuf;

pub fn app_data_root() -> PathBuf {
    if let Some(path) = env::var_os("RPOTATO_DATA_HOME") {
        return PathBuf::from(path);
    }

    if cfg!(target_os = "macos") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("rpotato");
        }
    }

    if cfg!(target_os = "windows") {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("rpotato");
        }
    }

    if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg_data_home).join("rpotato");
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("rpotato");
    }

    PathBuf::from(".rpotato-data")
}

pub fn managed_backend_path() -> PathBuf {
    let binary = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    app_data_root()
        .join("backends")
        .join("llama.cpp")
        .join(binary)
}

pub fn config_dir() -> PathBuf {
    app_data_root().join("config")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn backends_dir() -> PathBuf {
    app_data_root().join("backends")
}

pub fn models_dir() -> PathBuf {
    app_data_root().join("models")
}

pub fn model_registry_dir() -> PathBuf {
    models_dir().join("registry")
}

pub fn model_evidence_dir() -> PathBuf {
    models_dir().join("evidence")
}

pub fn model_default_file() -> PathBuf {
    models_dir().join("default.json")
}

pub fn downloads_dir() -> PathBuf {
    app_data_root().join("downloads")
}

pub fn manifests_dir() -> PathBuf {
    app_data_root().join("manifests")
}

pub fn logs_dir() -> PathBuf {
    app_data_root().join("logs")
}

pub fn operation_log_file() -> PathBuf {
    logs_dir().join("operation.log")
}

pub fn state_dir() -> PathBuf {
    app_data_root().join("state")
}

pub fn current_state_file() -> PathBuf {
    state_dir().join("current-state.json")
}

pub fn current_state_transition_lock() -> PathBuf {
    state_dir().join("current-state.transition.lock")
}

pub fn current_state_v2_promotion_temp() -> PathBuf {
    state_dir().join("current-state.json.v2-promote.tmp")
}

pub fn runtime_evidence_file() -> PathBuf {
    state_dir().join("runtime-evidence.jsonl")
}

pub fn validation_gaps_file() -> PathBuf {
    project_state_dir().join("validation-gaps.jsonl")
}

pub fn observability_db_file() -> PathBuf {
    state_dir().join("observability.sqlite")
}

pub fn runtime_ledger_file() -> PathBuf {
    state_dir().join("runtime-ledger.jsonl")
}

pub fn runtime_ledger_writer_lock() -> PathBuf {
    state_dir().join("runtime-ledger.writer.lock")
}

pub fn transcripts_dir() -> PathBuf {
    state_dir().join("transcripts")
}

pub fn transcript_session_dir(project_id: &str, session_id: &str) -> PathBuf {
    transcripts_dir().join(project_id).join(session_id)
}

pub fn transcript_file(project_id: &str, session_id: &str, record_id: &str) -> PathBuf {
    transcript_session_dir(project_id, session_id).join(format!("{record_id}.json"))
}

pub fn tool_outputs_dir() -> PathBuf {
    state_dir().join("tool-output")
}

pub fn tool_output_workflow_dir(project_id: &str, session_id: &str, workflow_id: &str) -> PathBuf {
    tool_outputs_dir()
        .join(project_id)
        .join(session_id)
        .join(workflow_id)
}

pub fn tool_output_file(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
) -> PathBuf {
    tool_output_workflow_dir(project_id, session_id, workflow_id)
        .join(format!("{artifact_id}.json"))
}

pub fn projection_lag_dir() -> PathBuf {
    state_dir().join("projection-lag")
}

pub fn projection_lag_file(intent_id: &str, event_id: &str) -> PathBuf {
    projection_lag_dir().join(format!("{intent_id}-{event_id}.json"))
}

pub fn plugins_dir() -> PathBuf {
    app_data_root().join("plugins")
}

pub fn imported_plugins_dir() -> PathBuf {
    plugins_dir().join("imported")
}

pub fn plugin_data_dir() -> PathBuf {
    plugins_dir().join("data")
}

pub fn cache_dir() -> PathBuf {
    app_data_root().join("cache")
}

pub fn project_state_dir() -> PathBuf {
    project_root().join(".rpotato")
}

pub fn project_root() -> PathBuf {
    env::var_os("RPOTATO_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn project_evidence_dir() -> PathBuf {
    project_state_dir().join("evidence")
}

pub fn project_session_ledger_file() -> PathBuf {
    project_state_dir().join("session-ledger.jsonl")
}

pub fn project_patch_proposals_dir() -> PathBuf {
    project_state_dir().join("patch-proposals")
}

pub fn project_transition_journal_dir(project_id: &str) -> PathBuf {
    project_state_dir()
        .join("transition-journal")
        .join(project_id)
}

pub fn project_transition_journal_file(project_id: &str, intent_id: &str) -> PathBuf {
    project_transition_journal_dir(project_id).join(format!("{intent_id}.prepared.json"))
}

pub fn project_transition_journal_temp(project_id: &str, intent_id: &str) -> PathBuf {
    project_transition_journal_dir(project_id).join(format!("{intent_id}.prepared.json.tmp"))
}

pub fn project_transition_lock(project_id: &str) -> PathBuf {
    project_transition_journal_dir(project_id).join("transition.lock")
}

pub fn project_workflows_dir() -> PathBuf {
    project_state_dir().join("workflows")
}

pub fn project_workflow_file(workflow_id: &str) -> PathBuf {
    project_workflows_dir().join(format!("{workflow_id}.json"))
}

pub fn project_workflow_snapshots_dir(workflow_id: &str) -> PathBuf {
    project_workflows_dir().join(format!("{workflow_id}.snapshots"))
}

pub fn project_workflow_snapshot_file(workflow_id: &str, revision: u64) -> PathBuf {
    project_workflow_snapshots_dir(workflow_id).join(format!("{revision:020}.json"))
}

pub fn project_workflow_transaction_file(workflow_id: &str) -> PathBuf {
    project_workflows_dir().join(format!("{workflow_id}.txn"))
}

pub fn project_subagents_dir() -> PathBuf {
    project_state_dir().join("subagents")
}

pub fn project_subagent_file(subagent_id: &str) -> PathBuf {
    project_subagents_dir().join(format!("{subagent_id}.json"))
}

pub fn project_subagent_snapshots_dir(subagent_id: &str) -> PathBuf {
    project_subagents_dir().join(format!("{subagent_id}.snapshots"))
}

pub fn project_subagent_snapshot_file(subagent_id: &str, revision: u64) -> PathBuf {
    project_subagent_snapshots_dir(subagent_id).join(format!("{revision:020}.json"))
}

pub fn project_subagent_lock(subagent_id: &str) -> PathBuf {
    project_subagents_dir().join(format!("{subagent_id}.lock"))
}

pub fn project_subagent_parent_lock(workflow_id: &str) -> PathBuf {
    project_subagents_dir().join(format!("parent-{workflow_id}.lock"))
}

pub fn project_subagent_execution_lock(subagent_id: &str) -> PathBuf {
    project_subagents_dir().join(format!("{subagent_id}.execution.lock"))
}

pub fn project_subagent_results_dir() -> PathBuf {
    project_state_dir().join("subagent-results")
}

pub fn project_subagent_result_file(result_artifact_id: &str) -> PathBuf {
    project_subagent_results_dir().join(format!("{result_artifact_id}.json"))
}

pub fn project_teams_dir() -> PathBuf {
    project_state_dir().join("teams")
}

pub fn project_team_file(team_id: &str) -> PathBuf {
    project_teams_dir().join(format!("{team_id}.json"))
}

pub fn project_team_manifest_file(team_id: &str) -> PathBuf {
    project_teams_dir().join(format!("{team_id}.manifest.json"))
}

pub fn project_team_snapshots_dir(team_id: &str) -> PathBuf {
    project_teams_dir().join(format!("{team_id}.snapshots"))
}

pub fn project_team_snapshot_file(team_id: &str, revision: u64) -> PathBuf {
    project_team_snapshots_dir(team_id).join(format!("{revision:020}.json"))
}

pub fn project_team_lock(team_id: &str) -> PathBuf {
    project_teams_dir().join(format!("{team_id}.lock"))
}

pub fn project_approval_requests_dir() -> PathBuf {
    project_state_dir().join("approval-requests")
}

pub fn project_ontology_dir() -> PathBuf {
    project_state_dir().join("ontology")
}

pub fn project_ontology_store_file() -> PathBuf {
    project_ontology_dir().join("graph.jsonl")
}

pub fn project_ontology_schema_file() -> PathBuf {
    project_ontology_dir().join("schema.json")
}
