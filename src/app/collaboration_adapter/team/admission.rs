use crate::adapters::filesystem::layout as paths;
use crate::app::approval_adapter as approval;
use crate::app::policy_adapter as policy;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team::{
    decision_label, OwnershipClaim, OwnershipGate, PolicyCheck, PolicyGate,
};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordedApprovalRequest {
    pub(super) request_id: String,
    pub(super) path: PathBuf,
}

pub(super) fn classify_policy_inputs(
    write_paths: &[String],
    commands: &[String],
) -> Result<Vec<PolicyCheck>, AppError> {
    let mut checks = Vec::new();
    for path in write_paths {
        let decision = policy::classify_path(policy::PathMode::Write, path)?;
        checks.push(PolicyCheck {
            target_type: "write",
            target: path.clone(),
            decision: decision.decision,
            class: decision.command_class,
            approval_prompt: decision.approval_prompt,
            reason: decision.reason,
        });
    }
    for command in commands {
        let decision = policy::classify_command(command)?;
        checks.push(PolicyCheck {
            target_type: "command",
            target: ledger::redact_text(command),
            decision: decision.decision,
            class: decision.command_class,
            approval_prompt: decision.approval_prompt,
            reason: decision.reason,
        });
    }

    Ok(checks)
}

pub(super) fn normalize_ownership_claims(
    owned_write_paths: &[(u32, String)],
) -> Result<Vec<OwnershipClaim>, AppError> {
    let mut claims = Vec::new();
    for (lane, raw_path) in owned_write_paths {
        let normalized_path = normalize_ownership_path(raw_path)?;
        claims.push(OwnershipClaim {
            lane: *lane,
            raw_path: raw_path.clone(),
            normalized_path,
        });
    }
    Ok(claims)
}

fn normalize_ownership_path(raw_path: &str) -> Result<String, AppError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(
            "team admit의 owned write path는 비어 있을 수 없습니다.",
        ));
    }
    let path = Path::new(trimmed);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Ok(trimmed.to_string());
    }

    let project_root = canonical_project_root()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let normalized = normalize_existing_or_parent(&candidate)?;
    let relative = normalized
        .strip_prefix(&project_root)
        .unwrap_or(&normalized)
        .to_path_buf();
    Ok(path_key(&relative))
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    std::fs::create_dir_all(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 만들지 못했습니다: {} ({err})",
            root.display()
        ))
    })?;
    std::fs::canonicalize(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({err})",
            root.display()
        ))
    })
}

fn normalize_existing_or_parent(path: &Path) -> Result<PathBuf, AppError> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|err| {
            AppError::runtime(format!(
                "path를 canonicalize하지 못했습니다: {} ({err})",
                path.display()
            ))
        });
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = std::fs::canonicalize(parent).map_err(|err| {
        AppError::runtime(format!(
            "path parent를 canonicalize하지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    Ok(canonical_parent.join(path.file_name().unwrap_or_default()))
}

fn path_key(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            Component::RootDir => Some(String::new()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn record_approval_request(
    identity: &ledger::RuntimeIdentity,
    event: &ledger::LedgerEvent,
    admission_status: &str,
    policy_gate: &PolicyGate,
    ownership_gate: &OwnershipGate,
) -> Result<Option<RecordedApprovalRequest>, AppError> {
    if !policy_gate.is_blocked() && !ownership_gate.is_blocked() {
        return Ok(None);
    }

    let request_id = format!("team-{}", event.event_id);
    let mut items = Vec::new();
    items.extend(
        policy_gate
            .checks
            .iter()
            .filter(|check| check.decision != policy::Decision::Allow)
            .map(|check| {
                format!(
                    "policy {}: {} -> {} ({}, approval: {}, reason: {})",
                    check.target_type,
                    check.target,
                    decision_label(check.decision),
                    check.class,
                    check.approval_prompt,
                    check.reason
                )
            }),
    );
    items.extend(
        ownership_gate
            .checks
            .iter()
            .filter(|check| check.status != "assigned")
            .map(|check| {
                format!(
                    "ownership lane {}: {} -> {} (normalized: {}, reason: {})",
                    check.lane, check.raw_path, check.status, check.normalized_path, check.reason
                )
            }),
    );
    if items.is_empty() {
        items.push("team admission blocked; inspect ledger event for details".to_string());
    }

    let status = if policy_gate.status == "approval-required" && !ownership_gate.is_blocked() {
        "pending-approval"
    } else {
        "blocked"
    };
    let path = approval::write_request(&approval::ApprovalRequest {
        request_id: request_id.clone(),
        source: "team-admission".to_string(),
        status: status.to_string(),
        reason: admission_status.to_string(),
        event_id: event.event_id.clone(),
        session_id: identity.session_id.clone(),
        summary: event.summary.clone(),
        items,
    })?;

    Ok(Some(RecordedApprovalRequest { request_id, path }))
}
