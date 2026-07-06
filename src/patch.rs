use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::app::AppError;
use crate::paths;
use crate::policy::{self, Decision, PathMode};
use crate::state;

const MAX_PATCH_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatchPreview {
    proposal_id: String,
    approval_token: String,
    relative_path: String,
    original_sha256: String,
    proposed_sha256: String,
    replacements: usize,
    diff: String,
    proposal_path: PathBuf,
}

pub fn preview_report(path: &str, find: &str, replace: &str) -> Result<String, AppError> {
    let preview = build_preview(path, find, replace)?;
    write_proposal_record(&preview)?;
    let event_id = state::record_event(
        "patch.preview.prepared",
        "patch diff preview prepared",
        &format!(
            "proposal_id={} path={} replacements={} original_sha256={} proposed_sha256={} proposal_path={}",
            preview.proposal_id,
            preview.relative_path,
            preview.replacements,
            preview.original_sha256,
            preview.proposed_sha256,
            preview.proposal_path.display()
        ),
    )?;

    Ok(format!(
        "patch preview\n- status: diff-ready\n- path: {}\n- proposal id: {}\n- replacements: {}\n- original sha256: {}\n- proposed sha256: {}\n- approval required: yes\n- approval token: {}\n- approval command: rpotato patch approve {} --token {} --dry-run\n- proposal record: {}\n- write gate: diff-before-write\n- ledger event: {}\n- boundary: 대상 파일은 수정하지 않았습니다. v0.3.0 approve는 gate 확인만 수행하고 patch apply는 후속 phase입니다.\n- diff:\n{}",
        preview.relative_path,
        preview.proposal_id,
        preview.replacements,
        preview.original_sha256,
        preview.proposed_sha256,
        preview.approval_token,
        preview.proposal_id,
        preview.approval_token,
        preview.proposal_path.display(),
        event_id,
        preview.diff
    ))
}

pub fn approve_report(proposal_id: &str, token: &str, dry_run: bool) -> Result<String, AppError> {
    if !dry_run {
        return Err(AppError::usage(
            "v0.3.0 patch approve는 --dry-run gate 확인만 허용합니다.",
        ));
    }
    validate_proposal_id(proposal_id)?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = fs::read_to_string(&proposal_path).map_err(|err| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record를 읽지 못했습니다.\n- proposal id: {}\n- path: {}\n- error: {}",
            proposal_id,
            proposal_path.display(),
            err
        ))
    })?;
    let expected = proposal_record_value(&record, "approval_token").ok_or_else(|| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record에 approval_token이 없습니다.\n- path: {}",
            proposal_path.display()
        ))
    })?;
    if expected != token {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: approval token 불일치\n- proposal id: {}\n- approval prompt: 사용자 승인 필요",
            proposal_id
        )));
    }
    let target_path =
        proposal_record_value(&record, "path").unwrap_or_else(|| "unknown".to_string());
    let event_id = state::record_event(
        "patch.approval.gate.passed",
        "patch approval gate passed",
        &format!(
            "proposal_id={} path={} dry_run={} proposal_path={}",
            proposal_id,
            target_path,
            dry_run,
            proposal_path.display()
        ),
    )?;

    Ok(format!(
        "patch approve\n- status: gate-passed\n- proposal id: {}\n- path: {}\n- dry-run: {}\n- approval token: accepted\n- proposal record: {}\n- ledger event: {}\n- boundary: approval gate만 확인했습니다. v0.3.0은 대상 파일 수정과 patch apply를 수행하지 않습니다.",
        proposal_id,
        target_path,
        dry_run,
        proposal_path.display(),
        event_id
    ))
}

fn build_preview(path: &str, find: &str, replace: &str) -> Result<PatchPreview, AppError> {
    if find.is_empty() {
        return Err(AppError::usage(
            "patch preview의 --find 값은 비어 있을 수 없습니다.",
        ));
    }
    let target = resolve_target(path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    if read_decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: target read policy가 allow가 아닙니다.\n- path: {}\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: target write policy가 deny입니다.\n- path: {}",
            target.relative_path
        )));
    }
    let metadata = fs::metadata(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(format!(
            "patch preview 대상은 file이어야 합니다: {}",
            target.relative_path
        )));
    }
    if metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: 대상 파일이 preview 한도를 초과했습니다.\n- path: {}\n- size bytes: {}\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            MAX_PATCH_FILE_BYTES
        )));
    }
    let original = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let matches = original.matches(find).count();
    if matches == 0 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text를 대상 파일에서 찾지 못했습니다.\n- path: {}",
            target.relative_path
        )));
    }
    if matches > 1 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text가 여러 번 나타나 patch target이 모호합니다.\n- path: {}\n- matches: {}",
            target.relative_path, matches
        )));
    }
    let proposed = original.replacen(find, replace, 1);
    if proposed == original {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: proposed content가 original과 동일합니다.\n- path: {}",
            target.relative_path
        )));
    }

    let original_sha256 = sha256_text(&original);
    let proposed_sha256 = sha256_text(&proposed);
    let diff = render_unified_diff(&target.relative_path, &original, &proposed);
    let proposal_id = format!(
        "patch-proposal-{}",
        &sha256_text(&format!(
            "{}\n{}\n{}",
            target.relative_path, original_sha256, proposed_sha256
        ))[..16]
    );
    let approval_token = sha256_text(&format!("{proposal_id}\n{diff}"))[..24].to_string();
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));

    Ok(PatchPreview {
        proposal_id,
        approval_token,
        relative_path: target.relative_path,
        original_sha256,
        proposed_sha256,
        replacements: matches,
        diff,
        proposal_path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetPath {
    absolute_path: PathBuf,
    relative_path: String,
}

fn resolve_target(raw_path: &str) -> Result<TargetPath, AppError> {
    if raw_path.trim().is_empty() {
        return Err(AppError::usage(
            "patch preview는 비어 있지 않은 --path 값이 필요합니다.",
        ));
    }
    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let raw = Path::new(raw_path);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        project_root.join(raw)
    };
    let absolute_path = fs::canonicalize(&candidate).map_err(|err| {
        AppError::runtime(format!(
            "patch preview 대상 path를 해석하지 못했습니다: {} ({err})",
            candidate.display()
        ))
    })?;
    let relative_path = absolute_path
        .strip_prefix(&project_root)
        .map_err(|_| {
            AppError::blocked(format!(
                "patch preview 차단\n- 이유: project boundary 밖 path입니다.\n- path: {}",
                raw_path
            ))
        })?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(TargetPath {
        absolute_path,
        relative_path,
    })
}

fn write_proposal_record(preview: &PatchPreview) -> Result<(), AppError> {
    if let Some(parent) = preview.proposal_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "patch proposal directory를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }
    fs::write(
        &preview.proposal_path,
        format!(
            "proposal_id={}\npath={}\napproval_token={}\noriginal_sha256={}\nproposed_sha256={}\nreplacements={}\n\n{}\n",
            preview.proposal_id,
            preview.relative_path,
            preview.approval_token,
            preview.original_sha256,
            preview.proposed_sha256,
            preview.replacements,
            preview.diff
        ),
    )
    .map_err(|err| {
        AppError::runtime(format!(
            "patch proposal record를 쓰지 못했습니다: {} ({err})",
            preview.proposal_path.display()
        ))
    })
}

fn render_unified_diff(path: &str, original: &str, proposed: &str) -> String {
    let old_lines = original.split('\n').collect::<Vec<_>>();
    let new_lines = proposed.split('\n').collect::<Vec<_>>();
    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix + prefix < old_lines.len()
        && suffix + prefix < new_lines.len()
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let context_before = prefix.saturating_sub(3);
    let context_after_old = (old_lines.len() - suffix + 3).min(old_lines.len());
    let context_after_new = (new_lines.len() - suffix + 3).min(new_lines.len());
    let old_start = context_before + 1;
    let new_start = context_before + 1;
    let old_count = context_after_old.saturating_sub(context_before).max(1);
    let new_count = context_after_new.saturating_sub(context_before).max(1);

    let mut diff = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -{},{} +{},{} @@\n",
        old_start, old_count, new_start, new_count
    );
    for line in &old_lines[context_before..prefix] {
        diff.push_str(&format!(" {line}\n"));
    }
    for line in &old_lines[prefix..old_lines.len() - suffix] {
        diff.push_str(&format!("-{line}\n"));
    }
    for line in &new_lines[prefix..new_lines.len() - suffix] {
        diff.push_str(&format!("+{line}\n"));
    }
    for line in &old_lines[old_lines.len() - suffix..context_after_old] {
        diff.push_str(&format!(" {line}\n"));
    }
    diff
}

fn proposal_record_value(record: &str, key: &str) -> Option<String> {
    record.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        if candidate == key {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn validate_proposal_id(proposal_id: &str) -> Result<(), AppError> {
    if proposal_id.starts_with("patch-proposal-")
        && proposal_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Ok(());
    }

    Err(AppError::usage(
        "patch approve proposal id 형식이 올바르지 않습니다.",
    ))
}

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let bytes = hasher.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn read_decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Allow => "allow",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_creates_diff_record_without_modifying_target() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-patch-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        let target = project_root.join("src/lib.rs");
        fs::write(&target, "fn answer() -> i32 {\n    1\n}\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "    1", "    2").unwrap();
        let contents = fs::read_to_string(&target).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(contents, "fn answer() -> i32 {\n    1\n}\n");
        assert!(report.contains("status: diff-ready"));
        assert!(report.contains("-    1"));
        assert!(report.contains("+    2"));
        assert!(report.contains("approval command: rpotato patch approve"));
    }

    #[test]
    fn approve_accepts_recorded_token_in_dry_run() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-approve-test-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&report, "proposal id").unwrap();
        let token = report_value(&report, "approval token").unwrap();
        let approval = approve_report(&proposal_id, &token, true).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(approval.contains("status: gate-passed"));
        assert!(approval.contains("boundary: approval gate만 확인했습니다"));
    }

    #[test]
    fn preview_blocks_ambiguous_find_text() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-patch-ambiguous-{}", std::process::id()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        fs::write(project_root.join("file.txt"), "same\nsame\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let err = preview_report("file.txt", "same", "changed").unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("여러 번"));
    }

    fn report_value(report: &str, key: &str) -> Option<String> {
        let prefix = format!("- {key}: ");
        report
            .lines()
            .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
    }
}
