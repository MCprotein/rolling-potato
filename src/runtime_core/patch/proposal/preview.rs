use crate::foundation::error::AppError;

use super::encoding::sha256_text;
use super::types::{PatchPreview, PreviewInput, MAX_PATCH_FILE_BYTES};

pub(crate) fn build_preview(input: PreviewInput<'_>) -> Result<PatchPreview, AppError> {
    if input.find.is_empty() {
        return Err(AppError::usage(
            "patch preview의 --find 값은 비어 있을 수 없습니다.",
        ));
    }
    let matches = input.original.matches(input.find).count();
    if matches == 0 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text를 대상 파일에서 찾지 못했습니다.\n- path: {}",
            input.relative_path
        )));
    }
    if matches > 1 {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: --find text가 여러 번 나타나 patch target이 모호합니다.\n- path: {}\n- matches: {}",
            input.relative_path, matches
        )));
    }
    let proposed = input.original.replacen(input.find, input.replace, 1);
    if proposed.len() > usize::try_from(MAX_PATCH_FILE_BYTES).expect("patch limit fits usize") {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: proposed content가 preview 한도를 초과했습니다.\n- path: {}\n- size bytes: {}\n- max bytes: {}",
            input.relative_path,
            proposed.len(),
            MAX_PATCH_FILE_BYTES
        )));
    }
    if proposed == input.original {
        return Err(AppError::blocked(format!(
            "patch preview 차단\n- 이유: proposed content가 original과 동일합니다.\n- path: {}",
            input.relative_path
        )));
    }

    let original_sha256 = sha256_text(input.original);
    let proposed_sha256 = sha256_text(&proposed);
    let diff = render_unified_diff(input.relative_path, input.original, &proposed);
    let content_id = &sha256_text(&format!(
        "{}\n{}\n{}",
        input.relative_path, original_sha256, proposed_sha256
    ))[..16];
    let proposal_id = if input.workflow_id.is_empty() {
        format!("patch-proposal-standalone-{content_id}")
    } else {
        format!(
            "patch-proposal-wf-{}-act-{}-{content_id}",
            safe_id_tail(input.workflow_id),
            safe_id_tail(input.action_id)
        )
    };

    let proposal_path = input.proposal_dir.join(format!("{proposal_id}.txt"));
    Ok(PatchPreview {
        proposal_id,
        approval_token: input.approval_token,
        relative_path: input.relative_path.to_string(),
        original_sha256,
        proposed_sha256,
        replacements: matches,
        diff,
        proposal_path,
        proposed_content: proposed,
        workflow_id: input.workflow_id.to_string(),
        action_id: input.action_id.to_string(),
        verification_command: input.verification_command.to_string(),
    })
}

fn safe_id_tail(value: &str) -> &str {
    value.rsplit('-').next().unwrap_or(value)
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
