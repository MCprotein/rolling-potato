//! Patch proposal, preview, and canonical record ownership.

use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, fmt::Write as _};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;

pub(crate) const MAX_PATCH_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchPreview {
    pub proposal_id: String,
    pub approval_token: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub replacements: usize,
    pub diff: String,
    pub proposal_path: PathBuf,
    pub proposed_content: String,
    pub workflow_id: String,
    pub action_id: String,
    pub verification_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProposalRecord {
    pub proposal_id: String,
    pub approval_token_hash: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub proposed_content: String,
    pub proposal_path: PathBuf,
    pub workflow_id: String,
    pub action_id: String,
    pub verification_command: String,
    pub artifact_hash: String,
    pub legacy_plaintext_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowProposal {
    pub proposal_id: String,
    pub approval_token: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub diff: String,
    pub verification_command: String,
    pub proposal_hash: String,
    pub approval_credential_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalSummary {
    pub proposal_id: String,
    pub relative_path: String,
    pub original_sha256: String,
    pub proposed_sha256: String,
    pub replacements: String,
    pub status: String,
    pub proposal_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposalDetail {
    pub summary: PatchProposalSummary,
    pub diff: String,
}

pub(crate) struct PreviewInput<'a> {
    pub relative_path: &'a str,
    pub original: &'a str,
    pub find: &'a str,
    pub replace: &'a str,
    pub workflow_id: &'a str,
    pub action_id: &'a str,
    pub verification_command: &'a str,
    pub approval_token: String,
    pub proposal_dir: &'a Path,
}

pub(crate) enum RecordParse {
    Canonical(ProposalRecord),
    LegacyMigration { scrubbed: String },
}

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

pub(crate) fn render_record(preview: &PatchPreview) -> String {
    format!(
        "record_version=4\nproposal_id={}\nworkflow_id={}\naction_id={}\npath={}\napproval_token_hash={}\noriginal_sha256={}\nproposed_sha256={}\nverification_command_hex={}\nreplacements={}\ncontent_encoding=utf8-hex\nproposed_content_hex={}\n\n{}\n",
        preview.proposal_id,
        preview.workflow_id,
        preview.action_id,
        preview.relative_path,
        sha256_text(&preview.approval_token),
        preview.original_sha256,
        preview.proposed_sha256,
        encode_hex_text(&preview.verification_command),
        preview.replacements,
        encode_hex_text(&preview.proposed_content),
        preview.diff
    )
}

pub(crate) fn parse_record(
    proposal_id: &str,
    proposal_path: &Path,
    contents: &str,
    allow_legacy_migration: bool,
) -> Result<RecordParse, AppError> {
    let (header, _) = parse_header(contents, proposal_path)?;
    let recorded_id = required_header(&header, "proposal_id", proposal_path)?;
    if recorded_id != proposal_id {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal id가 record와 일치하지 않습니다.\n- requested: {}\n- recorded: {}",
            proposal_id, recorded_id
        )));
    }
    let proposed_sha256 = required_header(&header, "proposed_sha256", proposal_path)?;
    let proposed_content_hex =
        required_header(&header, "proposed_content_hex", proposal_path).map_err(|_| {
            AppError::blocked(format!(
                "patch approve 차단\n- 이유: v0.4.0 apply에는 proposed_content_hex가 필요합니다.\n- path: {}\n- 동작: patch preview를 다시 생성하세요.",
                proposal_path.display()
            ))
        })?;
    let proposed_content = decode_hex_text(&proposed_content_hex).map_err(|message| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record의 proposed_content_hex를 해석하지 못했습니다.\n- path: {}\n- error: {}",
            proposal_path.display(),
            message
        ))
    })?;
    let decoded_sha256 = sha256_text(&proposed_content);
    if decoded_sha256 != proposed_sha256 {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record의 proposed content hash가 일치하지 않습니다.\n- expected: {}\n- actual: {}",
            proposed_sha256, decoded_sha256
        )));
    }

    let version = required_header(&header, "record_version", proposal_path)?;
    let legacy_plaintext_token = version == "2";
    if !matches!(version.as_str(), "2" | "4") {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: 지원하지 않는 proposal record version입니다.",
        ));
    }
    if legacy_plaintext_token {
        if !allow_legacy_migration {
            return Err(AppError::blocked(
                "legacy proposal read 차단\n- 동작: read-only/resume 경계에서 proposal을 변경하지 않았습니다.",
            ));
        }
        if header.contains_key("approval_token_hash") {
            return Err(AppError::blocked(
                "proposal strict parse 차단\n- 이유: v2 record에 hash credential이 함께 존재합니다.",
            ));
        }
        let plaintext = required_header(&header, "approval_token", proposal_path)?;
        let scrubbed = contents
            .replacen("record_version=2", "record_version=4", 1)
            .replacen(
                &format!("approval_token={plaintext}"),
                &format!("approval_token_hash={}", sha256_text(&plaintext)),
                1,
            );
        return Ok(RecordParse::LegacyMigration { scrubbed });
    }
    if header.contains_key("approval_token") {
        return Err(AppError::blocked(
            "proposal strict parse 차단\n- 이유: v4 record에 plaintext credential이 존재합니다.",
        ));
    }
    let approval_token_hash = required_header(&header, "approval_token_hash", proposal_path)?;
    Ok(RecordParse::Canonical(ProposalRecord {
        proposal_id: recorded_id,
        approval_token_hash,
        relative_path: required_header(&header, "path", proposal_path)?,
        original_sha256: required_header(&header, "original_sha256", proposal_path)?,
        proposed_sha256,
        proposed_content,
        proposal_path: proposal_path.to_path_buf(),
        workflow_id: header.get("workflow_id").cloned().unwrap_or_default(),
        action_id: header.get("action_id").cloned().unwrap_or_default(),
        verification_command: header
            .get("verification_command_hex")
            .cloned()
            .map(|value| decode_hex_text(&value))
            .transpose()
            .map_err(|message| {
                AppError::blocked(format!("verification plan decode 실패: {message}"))
            })?
            .unwrap_or_default(),
        artifact_hash: sha256_bytes(contents.as_bytes()),
        legacy_plaintext_token,
    }))
}

pub(crate) fn parse_header<'a>(
    contents: &'a str,
    path: &Path,
) -> Result<(BTreeMap<String, String>, &'a str), AppError> {
    const ALLOWED: &[&str] = &[
        "record_version",
        "proposal_id",
        "workflow_id",
        "action_id",
        "path",
        "approval_token_hash",
        "approval_token",
        "original_sha256",
        "proposed_sha256",
        "verification_command_hex",
        "replacements",
        "content_encoding",
        "proposed_content_hex",
    ];
    let (head, diff) = contents.split_once("\n\n").ok_or_else(|| {
        AppError::blocked(format!(
            "proposal strict parse 차단\n- path: {}\n- 이유: header terminator 없음",
            path.display()
        ))
    })?;
    let mut map = BTreeMap::new();
    for line in head.lines() {
        let (key, value) = line.split_once('=').ok_or_else(|| {
            AppError::blocked("proposal strict parse 차단\n- 이유: malformed field")
        })?;
        if !ALLOWED.contains(&key) {
            return Err(AppError::blocked(format!(
                "proposal strict parse 차단\n- 이유: unknown key: {key}"
            )));
        }
        if map.insert(key.to_string(), value.to_string()).is_some() {
            return Err(AppError::blocked(format!(
                "proposal strict parse 차단\n- 이유: duplicate key: {key}"
            )));
        }
    }
    Ok((map, diff))
}

pub(crate) fn required_header(
    map: &BTreeMap<String, String>,
    key: &str,
    path: &Path,
) -> Result<String, AppError> {
    map.get(key).cloned().ok_or_else(|| {
        AppError::blocked(format!(
            "patch approve 차단\n- 이유: proposal record에 {key} 값이 없습니다.\n- path: {}",
            path.display()
        ))
    })
}

pub(crate) fn validate_proposal_id(proposal_id: &str) -> Result<(), AppError> {
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

pub(crate) fn encode_hex_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(crate) fn decode_hex_text(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let chars = value.as_bytes();
    let mut index = 0usize;
    while index < chars.len() {
        let high = hex_value(chars[index]).ok_or_else(|| "invalid high nibble".to_string())?;
        let low = hex_value(chars[index + 1]).ok_or_else(|| "invalid low nibble".to_string())?;
        bytes.push((high << 4) | low);
        index += 2;
    }
    String::from_utf8(bytes).map_err(|err| err.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let bytes = hasher.finalize();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn sha256_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_record_bytes_and_preview_identity_are_stable() {
        let preview = build_preview(PreviewInput {
            relative_path: "src/lib.rs",
            original: "before\n",
            find: "before",
            replace: "after",
            workflow_id: "workflow-abc",
            action_id: "action-def",
            verification_command: "cargo test --locked",
            approval_token: "token".to_string(),
            proposal_dir: Path::new(""),
        })
        .unwrap();

        assert_eq!(
            preview.proposal_id,
            "patch-proposal-wf-abc-act-def-a8a4d19dc4eb6460"
        );
        assert_eq!(
            render_record(&preview),
            format!(
                "record_version=4\nproposal_id=patch-proposal-wf-abc-act-def-a8a4d19dc4eb6460\nworkflow_id=workflow-abc\naction_id=action-def\npath=src/lib.rs\napproval_token_hash={}\noriginal_sha256=9160d4be34c8695bd172a76c7c7966587ea5a4d991ad22c87b2b91af54aa9ebb\nproposed_sha256=7b9a72466d3960eb2aacccfc848939453490db0678bd4725def3f789b891c919\nverification_command_hex=636172676f2074657374202d2d6c6f636b6564\nreplacements=1\ncontent_encoding=utf8-hex\nproposed_content_hex=61667465720a\n\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-before\n+after\n \n\n",
                sha256_text("token")
            )
        );
    }

    #[test]
    fn preview_rejects_ambiguous_find_text() {
        let error = build_preview(PreviewInput {
            relative_path: "src/lib.rs",
            original: "same same",
            find: "same",
            replace: "other",
            workflow_id: "",
            action_id: "",
            verification_command: "",
            approval_token: String::new(),
            proposal_dir: Path::new(""),
        })
        .unwrap_err();

        assert_eq!(error.code, 3);
        assert!(error.message.contains("target이 모호합니다"));
    }
}
