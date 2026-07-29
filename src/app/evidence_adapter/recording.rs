use std::fs::{self, OpenOptions};
use std::io::Write;

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::knowledge::evidence::VerificationEvidence;

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
        crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())?;
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
