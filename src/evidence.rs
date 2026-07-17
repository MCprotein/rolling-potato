use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
pub use crate::runtime_core::knowledge::evidence::{
    stale_policy_summary, EvidenceStoreStatus, EvidenceValidation, VerificationEvidence,
};
use crate::runtime_core::knowledge::evidence::{
    validate_artifact_pointer_syntax, validate_stop_inputs, StopGateInputs,
};

pub fn record_patch_verification(
    workflow: &state::WorkflowRecord,
    command: &str,
    passed: bool,
    exit_code: &str,
    source_hash: &str,
    stdout: &str,
    stderr: &str,
) -> Result<VerificationEvidence, AppError> {
    let evidence_id = format!(
        "evidence-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            workflow.workflow_id, workflow.proposal_id, command, exit_code, source_hash
        ))[..20]
    );
    fs::create_dir_all(paths::project_evidence_dir())
        .map_err(|err| AppError::runtime(format!("evidence directory 생성 실패: {err}")))?;
    let payload = format!(
        "workflow_id={}\nproposal_id={}\naction_id={}\ncommand_hash={}\npassed={}\nexit_code={}\nsource_hash={}\nstdout_hash={}\nstderr_hash={}\n",
        workflow.workflow_id,
        workflow.proposal_id,
        workflow.action_id,
        state::sha256_text(command),
        passed,
        exit_code,
        source_hash,
        state::sha256_text(stdout),
        state::sha256_text(stderr)
    );
    let artifact_hash = state::sha256_text(&payload);
    let body = format!(
        "{{\n  \"schema_version\": 1,\n  \"evidence_id\": \"{}\",\n  \"artifact_hash\": \"{}\",\n  \"workflow_id\": \"{}\",\n  \"proposal_id\": \"{}\",\n  \"action_id\": \"{}\",\n  \"command_hash\": \"{}\",\n  \"passed\": {},\n  \"exit_code\": \"{}\",\n  \"source_hash\": \"{}\",\n  \"stdout_hash\": \"{}\",\n  \"stderr_hash\": \"{}\"\n}}\n",
        evidence_id,
        artifact_hash,
        workflow.workflow_id,
        workflow.proposal_id,
        workflow.action_id,
        state::sha256_text(command),
        passed,
        ledger::json_string(exit_code),
        source_hash,
        state::sha256_text(stdout),
        state::sha256_text(stderr)
    );
    let path = paths::project_evidence_dir().join(format!("{evidence_id}.json"));
    if path.exists() {
        let existing = fs::read_to_string(&path).map_err(|err| {
            AppError::blocked(format!(
                "verification evidence 기존 artifact 읽기 실패: {err}"
            ))
        })?;
        if existing != body {
            return Err(AppError::blocked("verification evidence 충돌\n- 이유: deterministic evidence id에 다른 artifact가 존재합니다."));
        }
    } else {
        state::atomic_replace_bytes(&path, body.as_bytes())?;
    }
    evidence_fault("after-artifact")?;
    let runtime_line = format!(
        "{{\"schema_version\":1,\"evidence_id\":\"{}\",\"workflow_id\":\"{}\",\"artifact_hash\":\"{}\",\"passed\":{},\"source_hash\":\"{}\"}}",
        evidence_id, workflow.workflow_id, artifact_hash, passed, source_hash
    );
    if let Some(parent) = paths::runtime_evidence_file().parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!("runtime evidence directory 생성 실패: {err}"))
        })?;
    }
    let runtime_path = paths::runtime_evidence_file();
    let existing_runtime = match fs::read_to_string(&runtime_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "runtime evidence 읽기 실패: {err}"
            )))
        }
    };
    let mut found = false;
    for line in existing_runtime.lines() {
        let object = strict_json::parse_object(
            line,
            &[
                "schema_version",
                "evidence_id",
                "workflow_id",
                "artifact_hash",
                "passed",
                "source_hash",
            ],
            "runtime evidence line",
        )?;
        if strict_json::number(&object, "schema_version", "runtime evidence line")? != 1 {
            return Err(AppError::blocked("runtime evidence schema version 불일치"));
        }
        if strict_json::string(&object, "evidence_id", "runtime evidence line")? == evidence_id {
            if line != runtime_line {
                return Err(AppError::blocked("runtime evidence deterministic id 충돌"));
            }
            found = true;
        }
    }
    if !found {
        let mut runtime = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&runtime_path)
            .map_err(|err| AppError::runtime(format!("runtime evidence open 실패: {err}")))?;
        writeln!(runtime, "{runtime_line}")
            .map_err(|err| AppError::runtime(format!("runtime evidence append 실패: {err}")))?;
        runtime
            .sync_all()
            .map_err(|err| AppError::runtime(format!("runtime evidence sync 실패: {err}")))?;
    }
    evidence_fault("after-runtime")?;
    if !ledger::event_detail_exists(
        "verification.evidence.recorded",
        "evidence_id",
        &evidence_id,
    )? {
        state::record_event(
            "verification.evidence.recorded",
            "patch verification evidence recorded",
            &format!(
                "workflow_id={} evidence_id={} artifact_hash={} passed={} source_hash={}",
                workflow.workflow_id, evidence_id, artifact_hash, passed, source_hash
            ),
        )?;
    }
    transcript::record_workflow_turn(
        workflow,
        "evidence",
        &evidence_id,
        &format!(
            "patch verification: evidence_id={} passed={} exit_code={} source_hash={} artifact_hash={} stdout_hash={} stderr_hash={}",
            evidence_id,
            passed,
            exit_code,
            source_hash,
            artifact_hash,
            state::sha256_text(stdout),
            state::sha256_text(stderr)
        ),
        &[],
    )?;
    evidence_fault("after-event")?;
    Ok(VerificationEvidence {
        evidence_id,
        artifact_hash,
        passed,
    })
}

pub fn evaluate_patch_stop_gate(workflow: &state::WorkflowRecord) -> Result<(), AppError> {
    validate_patch_stop_gate_inner(workflow, true)
}

pub fn validate_patch_stop_gate(workflow: &state::WorkflowRecord) -> Result<(), AppError> {
    validate_patch_stop_gate_inner(workflow, false)
}

fn validate_patch_stop_gate_inner(
    workflow: &state::WorkflowRecord,
    record_event: bool,
) -> Result<(), AppError> {
    let path = paths::project_evidence_dir().join(format!("{}.json", workflow.evidence_id));
    let body = fs::read_to_string(&path)
        .map_err(|_| stop_gate_error(workflow, "verification evidence missing", record_event))?;
    const KEYS: &[&str] = &[
        "schema_version",
        "evidence_id",
        "artifact_hash",
        "workflow_id",
        "proposal_id",
        "action_id",
        "command_hash",
        "passed",
        "exit_code",
        "source_hash",
        "stdout_hash",
        "stderr_hash",
    ];
    let object = strict_json::parse_object(&body, KEYS, "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?;
    if strict_json::number(&object, "schema_version", "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?
        != 1
    {
        return Err(stop_gate_error(
            workflow,
            "verification evidence schema version mismatch",
            record_event,
        ));
    }
    let field = |key| {
        strict_json::string(&object, key, "verification evidence")
            .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))
    };
    let evidence_id = field("evidence_id")?;
    let body_artifact_hash = field("artifact_hash")?;
    let evidence_workflow = field("workflow_id")?;
    let evidence_proposal = field("proposal_id")?;
    let evidence_action = field("action_id")?;
    let command_hash = field("command_hash")?;
    let exit_code = field("exit_code")?;
    let source_hash = field("source_hash")?;
    let stdout_hash = field("stdout_hash")?;
    let stderr_hash = field("stderr_hash")?;
    let passed = strict_json::boolean(&object, "passed", "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?;
    let payload = format!(
        "workflow_id={}\nproposal_id={}\naction_id={}\ncommand_hash={}\npassed={}\nexit_code={}\nsource_hash={}\nstdout_hash={}\nstderr_hash={}\n",
        evidence_workflow,
        evidence_proposal,
        evidence_action,
        command_hash,
        passed,
        exit_code,
        source_hash,
        stdout_hash,
        stderr_hash
    );
    let recomputed_hash = state::sha256_text(&payload);
    let source =
        fs::read_to_string(paths::project_root().join(&workflow.source_path)).map_err(|_| {
            stop_gate_error(workflow, "authoritative source reread failed", record_event)
        })?;
    let expected_command_hash = state::sha256_text(&workflow.verification_plan);
    let authoritative_source_hash = state::sha256_text(&source);
    if !validate_stop_inputs(&StopGateInputs {
        phase: &workflow.phase,
        approval_state: &workflow.approval_state,
        verification_approval_state: &workflow.verification_approval_state,
        expected_workflow_id: &workflow.workflow_id,
        expected_proposal_id: &workflow.proposal_id,
        expected_action_id: &workflow.action_id,
        expected_evidence_id: &workflow.evidence_id,
        expected_evidence_hash: &workflow.evidence_hash,
        expected_command_hash: &expected_command_hash,
        expected_source_hash: &workflow.after_hash,
        evidence_workflow_id: &evidence_workflow,
        evidence_proposal_id: &evidence_proposal,
        evidence_action_id: &evidence_action,
        evidence_id: &evidence_id,
        body_artifact_hash: &body_artifact_hash,
        recomputed_artifact_hash: &recomputed_hash,
        command_hash: &command_hash,
        source_hash: &source_hash,
        authoritative_source_hash: &authoritative_source_hash,
        passed,
    }) {
        return Err(stop_gate_error(
            workflow,
            "missing or stale applied/verification evidence",
            record_event,
        ));
    }
    if record_event {
        state::record_event(
            "workflow.stop_gate.passed",
            "workflow stop gate passed",
            &format!(
                "workflow_id={} proposal_id={} evidence_id={} applied_hash={} unresolved_approval=false",
                workflow.workflow_id, workflow.proposal_id, workflow.evidence_id, workflow.after_hash
            ),
        )?;
    }
    Ok(())
}

fn evidence_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_EVIDENCE_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected evidence crash: {point}"
        )));
    }
    Ok(())
}

fn stop_gate_error(workflow: &state::WorkflowRecord, reason: &str, record_event: bool) -> AppError {
    let persistence = if record_event {
        state::record_event(
            "workflow.stop_gate.failed",
            "workflow stop gate failed",
            &format!(
                "workflow_id={} reason={}",
                workflow.workflow_id,
                reason.replace(' ', "-")
            ),
        )
        .err()
        .map(|err| format!("\n- stop-gate failure event 저장 실패: {}", err.message))
        .unwrap_or_default()
    } else {
        String::new()
    };
    AppError::blocked(format!(
        "workflow stop gate 차단\n- workflow id: {}\n- 이유: {}\n- 동작: 성공 보고를 생성하지 않습니다.{}",
        workflow.workflow_id, reason, persistence
    ))
}

pub fn store_status() -> Result<EvidenceStoreStatus, AppError> {
    let runtime_evidence_file = paths::runtime_evidence_file();
    let project_evidence_dir = paths::project_evidence_dir();

    Ok(EvidenceStoreStatus {
        runtime_evidence_records: count_jsonl_records(&runtime_evidence_file)?,
        project_artifacts: count_files(&project_evidence_dir)?,
        runtime_evidence_file,
        project_evidence_dir,
        stale_policy: stale_policy_summary(),
        truncated: false,
    })
}

pub(crate) fn store_status_bounded(
    scan_limit: usize,
    max_bytes: u64,
) -> Result<EvidenceStoreStatus, AppError> {
    if scan_limit == 0 || max_bytes == 0 {
        return Err(AppError::blocked(
            "evidence read-only budget은 0보다 커야 합니다.",
        ));
    }
    let runtime_evidence_file = paths::runtime_evidence_file();
    let project_evidence_dir = paths::project_evidence_dir();
    let (runtime_evidence_records, runtime_truncated) =
        count_jsonl_records_bounded(&runtime_evidence_file, scan_limit, max_bytes)?;
    let (project_artifacts, project_truncated) =
        count_top_level_files_bounded(&project_evidence_dir, scan_limit)?;
    Ok(EvidenceStoreStatus {
        runtime_evidence_file,
        runtime_evidence_records,
        project_evidence_dir,
        project_artifacts,
        stale_policy: stale_policy_summary(),
        truncated: runtime_truncated || project_truncated,
    })
}

pub fn validate_report(pointer: &str) -> Result<String, AppError> {
    let validation = validate_artifact_pointer(pointer)?;
    Ok(format!(
        "evidence validate 결과\n- artifact: {}\n- project root: {}\n- boundary: project root 내부\n- stale policy: {}\n- 동작: artifact pointer가 존재하고 project boundary를 벗어나지 않는지 확인했습니다.",
        validation.artifact.display(),
        validation.project_root.display(),
        validation.stale_policy
    ))
}

pub fn validate_artifact_pointer(pointer: &str) -> Result<EvidenceValidation, AppError> {
    validate_artifact_pointer_syntax(pointer)?;
    let pointer_path = Path::new(pointer);
    let project_root = canonical_project_root()?;
    let artifact = project_root.join(pointer_path);
    if !artifact.exists() {
        return Err(AppError::usage(format!(
            "evidence artifact가 존재하지 않습니다: {}",
            artifact.display()
        )));
    }

    let canonical_artifact = fs::canonicalize(&artifact).map_err(|err| {
        AppError::runtime(format!(
            "evidence artifact를 canonicalize하지 못했습니다: {} ({err})",
            artifact.display()
        ))
    })?;

    if !canonical_artifact.starts_with(&project_root) {
        return Err(AppError::blocked(format!(
            "evidence artifact가 project boundary를 벗어났습니다: {}",
            canonical_artifact.display()
        )));
    }

    Ok(EvidenceValidation {
        artifact: canonical_artifact,
        project_root,
        stale_policy: stale_policy_summary(),
    })
}

fn count_jsonl_records(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let body = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "runtime evidence store를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(body.lines().filter(|line| !line.trim().is_empty()).count())
}

fn count_jsonl_records_bounded(
    path: &Path,
    scan_limit: usize,
    max_bytes: u64,
) -> Result<(usize, bool), AppError> {
    if !path.exists() {
        return Ok((0, false));
    }
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "runtime evidence metadata를 읽지 못했습니다: {err}"
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(
            "runtime evidence regular-file boundary 불일치",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|err| AppError::blocked(format!("runtime evidence open 실패: {err}")))?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("runtime evidence bounded read 실패: {err}")))?;
    let byte_truncated = bytes.len() as u64 > max_bytes;
    if byte_truncated {
        bytes.truncate(max_bytes as usize);
    }
    let body = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::blocked("runtime evidence UTF-8 불일치"))?;
    let mut count = 0_usize;
    let mut record_truncated = false;
    for line in body.lines().filter(|line| !line.trim().is_empty()) {
        if count == scan_limit {
            record_truncated = true;
            break;
        }
        count = count.saturating_add(1);
        strict_json::parse_value(line, "runtime evidence bounded record")?;
    }
    Ok((count, byte_truncated || record_truncated))
}

fn count_top_level_files_bounded(
    path: &Path,
    scan_limit: usize,
) -> Result<(usize, bool), AppError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok((0, false)),
        Err(err) => {
            return Err(AppError::blocked(format!(
                "project evidence directory 읽기 실패: {err}"
            )))
        }
    };
    let mut files = 0_usize;
    let mut scanned = 0_usize;
    for entry in entries {
        if scanned == scan_limit {
            return Ok((files, true));
        }
        scanned = scanned.saturating_add(1);
        let entry = entry
            .map_err(|err| AppError::blocked(format!("project evidence entry 실패: {err}")))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|err| AppError::blocked(format!("project evidence metadata 실패: {err}")))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked(
                "project evidence view는 top-level regular file만 허용합니다.",
            ));
        }
        files = files.saturating_add(1);
    }
    Ok((files, false))
}

fn count_files(path: &Path) -> Result<usize, AppError> {
    if !path.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in fs::read_dir(path).map_err(|err| {
        AppError::runtime(format!(
            "project evidence 디렉터리를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목을 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            AppError::runtime(format!(
                "project evidence 항목 타입을 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        if file_type.is_file() {
            count += 1;
        } else if file_type.is_dir() {
            count += count_files(&entry.path())?;
        }
    }
    Ok(count)
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    fs::create_dir_all(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 만들지 못했습니다: {} ({err})",
            root.display()
        ))
    })?;
    fs::canonicalize(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({err})",
            root.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_remote_artifact_pointer() {
        let err = validate_artifact_pointer("https://example.com/evidence.json")
            .expect_err("remote evidence pointers must be blocked");
        assert_eq!(err.code, 3);
    }

    #[test]
    fn rejects_parent_dir_artifact_pointer() {
        let err = validate_artifact_pointer("../outside.log")
            .expect_err("parent directory evidence pointers must be blocked");
        assert_eq!(err.code, 3);
    }

    #[test]
    fn store_status_counts_runtime_records_and_project_artifacts() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-evidence-store-test-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);

        fs::create_dir_all(paths::state_dir()).unwrap();
        fs::create_dir_all(paths::project_evidence_dir().join("nested")).unwrap();
        fs::write(
            paths::runtime_evidence_file(),
            "{\"evidence_id\":\"one\"}\n\n{\"evidence_id\":\"two\"}\n",
        )
        .unwrap();
        fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();
        fs::write(
            paths::project_evidence_dir().join("nested").join("two.txt"),
            "two",
        )
        .unwrap();

        let status = store_status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(status.runtime_evidence_records, 2);
        assert_eq!(status.project_artifacts, 2);
        assert_eq!(status.stale_policy, stale_policy_summary());
    }

    #[test]
    fn stop_gate_rejects_missing_and_stale_evidence() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-stop-gate-test-{}", std::process::id()));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("src")).unwrap();
        fs::create_dir_all(project.join(".rpotato/evidence")).unwrap();
        fs::write(project.join("src/lib.rs"), "after\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        let after_hash = state::sha256_text("after\n");
        let mut workflow = state::WorkflowRecord::new(&ledger::fresh_identity(), "test");
        workflow.phase = "verified".to_string();
        workflow.approval_state = "approved".to_string();
        workflow.proposal_id = "patch-proposal-test".to_string();
        workflow.source_path = "src/lib.rs".to_string();
        workflow.after_hash = after_hash.clone();
        workflow.evidence_id = "evidence-missing".to_string();
        workflow.evidence_hash = "expected".to_string();

        let missing = evaluate_patch_stop_gate(&workflow).unwrap_err();
        assert_eq!(missing.code, 3);

        fs::write(
            project.join(".rpotato/evidence/evidence-missing.json"),
            format!(
                "{{\"artifact_hash\": \"wrong\", \"workflow_id\": \"{}\", \"proposal_id\": \"{}\", \"source_hash\": \"{}\", \"passed\": true}}",
                workflow.workflow_id, workflow.proposal_id, after_hash
            ),
        )
        .unwrap();
        let stale = evaluate_patch_stop_gate(&workflow).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
        assert_eq!(stale.code, 3);
        assert!(stale.message.contains("malformed verification evidence"));
    }

    #[test]
    fn evidence_crash_after_event_is_idempotent_without_duplicate_receipts() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in ["after-artifact", "after-runtime", "after-event"] {
            let root = std::env::temp_dir().join(format!(
                "rpotato-evidence-dedupe-{point}-{}",
                std::process::id()
            ));
            let project = root.join("project");
            let data = root.join("data");
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(project.join("src")).unwrap();
            std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
            std::env::set_var("RPOTATO_DATA_HOME", &data);
            state::initialize().unwrap();
            let mut workflow =
                state::WorkflowRecord::new(&ledger::fresh_identity(), "evidence dedupe");
            workflow.proposal_id = "patch-proposal-evidence-test".to_string();
            workflow.action_id = "action-evidence-test".to_string();
            let source_hash = state::sha256_text("after\n");
            std::env::set_var("RPOTATO_TEST_EVIDENCE_FAULT", point);
            let injected =
                record_patch_verification(&workflow, "pwd", true, "0", &source_hash, "ok", "")
                    .unwrap_err();
            std::env::remove_var("RPOTATO_TEST_EVIDENCE_FAULT");
            let receipt =
                record_patch_verification(&workflow, "pwd", true, "0", &source_hash, "ok", "")
                    .unwrap();
            let runtime_records = fs::read_to_string(paths::runtime_evidence_file())
                .unwrap()
                .lines()
                .filter(|line| line.contains(&receipt.evidence_id))
                .count();
            let ledger_events = ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| {
                    event.event_type == "verification.evidence.recorded"
                        && event
                            .details
                            .contains(&format!("evidence_id={}", receipt.evidence_id))
                })
                .count();
            std::env::remove_var("RPOTATO_PROJECT_ROOT");
            std::env::remove_var("RPOTATO_DATA_HOME");
            let _ = fs::remove_dir_all(root);
            assert_eq!(injected.code, 1, "point: {point}");
            assert_eq!(runtime_records, 1, "point: {point}");
            assert_eq!(ledger_events, 1, "point: {point}");
        }
    }
}
