//! Durable transcript and historical-source context reconstruction.

use std::collections::BTreeSet;
use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::app::ontology_adapter as ontology;
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::compaction::{
    estimate_tokens, truncate_tail_to_estimated_tokens, CompactionPolicy,
};
use crate::runtime_core::knowledge::context::{
    truncate_chars, ContextPack, ResumeContext, ResumeContextBudget, SourcePointer,
    MAX_CONTEXT_CHARS, MAX_CONTEXT_FILES, MAX_FILE_BYTES, MAX_FILE_CHARS,
};

use super::compaction;

pub fn rebuild_resume_context(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
) -> Result<ResumeContext, AppError> {
    let context_limit_tokens =
        crate::app::inference_adapter::context_window::effective_context_window()?.limit_tokens
            as usize;
    rebuild_session_context_for_limit(
        session_id,
        exclude_workflow_id,
        context_limit_tokens,
        HistoricalSourcePolicy::Strict,
    )
}

pub fn build_active_conversation_context(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
) -> Result<ResumeContext, AppError> {
    let context_limit_tokens =
        crate::app::inference_adapter::context_window::effective_context_window()?.limit_tokens
            as usize;
    build_active_conversation_context_for_limit(
        session_id,
        exclude_workflow_id,
        context_limit_tokens,
    )
}

#[cfg(test)]
pub(crate) fn rebuild_resume_context_for_limit(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
    context_limit_tokens: usize,
) -> Result<ResumeContext, AppError> {
    rebuild_session_context_for_limit(
        session_id,
        exclude_workflow_id,
        context_limit_tokens,
        HistoricalSourcePolicy::Strict,
    )
}

pub(crate) fn build_active_conversation_context_for_limit(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
    context_limit_tokens: usize,
) -> Result<ResumeContext, AppError> {
    rebuild_session_context_for_limit(
        session_id,
        exclude_workflow_id,
        context_limit_tokens,
        HistoricalSourcePolicy::BestEffort,
    )
}

#[derive(Clone, Copy)]
enum HistoricalSourcePolicy {
    Strict,
    BestEffort,
}

fn rebuild_session_context_for_limit(
    session_id: &str,
    exclude_workflow_id: Option<&str>,
    context_limit_tokens: usize,
    source_policy: HistoricalSourcePolicy,
) -> Result<ResumeContext, AppError> {
    let budget = ResumeContextBudget::for_context_limit(context_limit_tokens);
    let records = transcript::records_for_session(session_id)?;
    let compacted = match source_policy {
        HistoricalSourcePolicy::Strict => compaction::load_current_artifact(session_id)?,
        HistoricalSourcePolicy::BestEffort => {
            compaction::load_current_artifact(session_id).ok().flatten()
        }
    };
    let boundary_index = compacted.as_ref().and_then(|artifact| {
        records
            .iter()
            .position(|record| record.record_id == artifact.boundary_record_id)
    });
    let compacted = boundary_index.zip(compacted);
    let eligible_records = compacted
        .as_ref()
        .map_or(records.as_slice(), |(index, _)| &records[index + 1..]);
    let eligible = eligible_records
        .iter()
        .filter(|record| exclude_workflow_id != Some(record.workflow_id.as_str()))
        .collect::<Vec<_>>();

    let mut selected_reversed = Vec::new();
    let mut transcript_tokens = 0usize;
    let mut transcript_chars = 0usize;
    for record in eligible.iter().rev() {
        if selected_reversed.len() >= budget.max_turns
            || transcript_tokens >= budget.transcript_budget_tokens
        {
            break;
        }
        let remaining = budget
            .transcript_budget_tokens
            .saturating_sub(transcript_tokens);
        let content = truncate_tail_to_estimated_tokens(
            &record.content,
            remaining.min(budget.per_turn_budget_tokens),
        );
        let tokens = estimate_tokens(&content);
        let chars = content.chars().count();
        if chars == 0 || tokens == 0 || tokens > remaining {
            continue;
        }
        transcript_tokens += tokens;
        transcript_chars += chars;
        selected_reversed.push((record.kind.clone(), content));
    }
    selected_reversed.reverse();

    let project_root = fs::canonicalize(paths::project_root()).map_err(|err| {
        AppError::runtime(format!(
            "project root를 해석하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let mut seen = BTreeSet::new();
    let mut pointers_reversed = Vec::new();
    let mut files_considered = 0usize;
    for record in eligible.iter().rev() {
        for pointer in record.source_pointers.iter().rev() {
            if pointers_reversed.len() >= MAX_CONTEXT_FILES {
                break;
            }
            if seen.insert(pointer.stable_ref.clone()) {
                files_considered += 1;
                pointers_reversed.push(pointer.clone());
            }
        }
        if pointers_reversed.len() >= MAX_CONTEXT_FILES {
            break;
        }
    }
    pointers_reversed.reverse();

    let mut source_pointers = Vec::new();
    let mut chars_read = 0usize;
    for pointer in pointers_reversed {
        if chars_read >= MAX_CONTEXT_CHARS {
            break;
        }
        let source = match source_policy {
            HistoricalSourcePolicy::Strict => Some(ontology::reread_runtime_source(
                &pointer.stable_ref,
                &pointer.source_hash,
            )?),
            HistoricalSourcePolicy::BestEffort => {
                ontology::reread_historical_source(&pointer.stable_ref, &pointer.source_hash)?
            }
        };
        let Some(source) = source else {
            continue;
        };
        if source.relative_path != pointer.path {
            return Err(AppError::blocked(format!(
                "resume source pointer binding 불일치\n- pointer: {}",
                pointer.stable_ref
            )));
        }
        if source.contents.len() as u64 > MAX_FILE_BYTES || source.contents.trim().is_empty() {
            continue;
        }
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(chars_read);
        let snippet = truncate_chars(&source.contents, remaining.min(MAX_FILE_CHARS));
        let chars = snippet.chars().count();
        chars_read += chars;
        source_pointers.push(SourcePointer {
            path: source.relative_path,
            stable_ref: source.stable_ref,
            chars,
            fingerprint: source.source_hash,
            snippet,
        });
    }

    let compaction_target_tokens = compacted
        .as_ref()
        .map(|(_, artifact)| usize::try_from(artifact.post_compact_target_tokens))
        .transpose()
        .map_err(|_| AppError::blocked("compaction target token count overflow"))?
        .map(|stored_target| {
            stored_target.min(
                CompactionPolicy::for_context_limit(budget.context_limit_tokens)
                    .post_compact_target_tokens,
            )
        });

    Ok(ResumeContext {
        session_id: session_id.to_string(),
        context_limit_tokens: budget.context_limit_tokens,
        transcript_records_considered: eligible.len(),
        transcript_turns_selected: selected_reversed.len(),
        transcript_tokens,
        transcript_chars,
        transcript: selected_reversed,
        compacted_checkpoint: compacted
            .as_ref()
            .map(|(_, artifact)| artifact.checkpoint.clone()),
        compaction_boundary: compacted
            .as_ref()
            .map(|(_, artifact)| artifact.boundary_record_id.clone()),
        compaction_target_tokens,
        sources: ContextPack {
            project_root,
            origin: "durable-transcript-source-pointers".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: 0,
            files_considered,
            files_read: source_pointers.len(),
            chars_read,
            dropped_files: files_considered.saturating_sub(source_pointers.len()),
            source_pointers,
        },
    })
}
