use std::collections::BTreeMap;
use std::path::Path;

use crate::foundation::error::AppError;

use super::super::approval::hash_token;
use super::encoding::{decode_hex_text, encode_hex_text, sha256_bytes, sha256_text};
use super::types::{PatchPreview, ProposalRecord, RecordParse};

pub(crate) fn render_record(preview: &PatchPreview) -> String {
    format!(
        "record_version=4\nproposal_id={}\nworkflow_id={}\naction_id={}\npath={}\napproval_token_hash={}\noriginal_sha256={}\nproposed_sha256={}\nverification_command_hex={}\nreplacements={}\ncontent_encoding=utf8-hex\nproposed_content_hex={}\n\n{}\n",
        preview.proposal_id,
        preview.workflow_id,
        preview.action_id,
        preview.relative_path,
        hash_token(&preview.approval_token),
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
                &format!("approval_token_hash={}", hash_token(&plaintext)),
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
    Ok(RecordParse::Canonical(Box::new(ProposalRecord {
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
    })))
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
