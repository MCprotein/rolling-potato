use std::path::PathBuf;

use super::super::compaction::{estimate_tokens, CompactionCheckpoint};
use super::{
    assemble_agent_prompt, AgentPromptBudget, AgentPromptParts, ContextPack, ResumeContext,
    ResumeContextBudget, SourcePointer,
};

#[test]
fn resume_budget_scales_with_the_declared_model_window() {
    let small = ResumeContextBudget::for_context_limit(4_096);
    let large = ResumeContextBudget::for_context_limit(131_072);

    assert_eq!(small.context_limit_tokens, 4_096);
    assert_eq!(small.transcript_budget_tokens, 512);
    assert_eq!(small.max_turns, 8);
    assert_eq!(large.context_limit_tokens, 131_072);
    assert_eq!(large.transcript_budget_tokens, 16_384);
    assert_eq!(large.per_turn_budget_tokens, 4_096);
    assert_eq!(large.max_turns, 64);
}

#[test]
fn compacted_resume_prompt_honors_one_total_budget_for_korean_content() {
    let korean = "작은 모델이 이어서 수행해야 하는 긴 한국어 컨텍스트 ".repeat(200);
    let checkpoint = CompactionCheckpoint {
        current_task: korean.clone(),
        constraints: (0..8)
            .map(|index| format!("제약 {index} {korean}"))
            .collect(),
        decisions: (0..8)
            .map(|index| format!("결정 {index} {korean}"))
            .collect(),
        remaining_work: (0..8)
            .map(|index| format!("남은 작업 {index} {korean}"))
            .collect(),
        ..CompactionCheckpoint::default()
    };
    let source_pointers = (0..4)
        .map(|index| SourcePointer {
            path: format!("src/file-{index}.rs"),
            stable_ref: format!("src/file-{index}.rs:1"),
            chars: korean.chars().count(),
            fingerprint: "a".repeat(64),
            snippet: korean.clone(),
        })
        .collect::<Vec<_>>();
    let resume = ResumeContext {
        session_id: "session-budget".to_string(),
        context_limit_tokens: 4_096,
        transcript_records_considered: 8,
        transcript_turns_selected: 8,
        transcript_tokens: estimate_tokens(&korean) * 8,
        transcript_chars: korean.chars().count() * 8,
        transcript: (0..8)
            .map(|index| ("user".to_string(), format!("turn-{index} {korean}")))
            .collect(),
        compacted_checkpoint: Some(checkpoint),
        compaction_boundary: Some("record-boundary".to_string()),
        compaction_target_tokens: Some(1_638),
        sources: ContextPack {
            project_root: PathBuf::from("/project"),
            origin: "test".to_string(),
            ontology_records_selected: 0,
            ontology_stale_rejected: 0,
            files_considered: 4,
            files_read: 4,
            chars_read: korean.chars().count() * 4,
            dropped_files: 0,
            source_pointers,
        },
    };

    let prompt = resume.prompt_section();

    assert!(estimate_tokens(&prompt) <= 1_638);
    assert!(prompt.contains("derived compacted checkpoint"));
    assert!(prompt.contains("[compacted]"));
    assert!(prompt.contains("repository context"));
}

#[test]
fn agent_prompt_stays_inside_a_1024_token_runtime_window_with_max_context() {
    let budget = AgentPromptBudget::for_context_limit(1_024, 256).unwrap();
    let resume = "이전 대화와 작업 상태 ".repeat(4_000);
    let repository = "저장소 소스 코드와 검증 근거 ".repeat(4_000);
    let assembled = assemble_agent_prompt(
        budget,
        AgentPromptParts {
            instructions: "필수 runtime 계약: 부작용을 실행하지 말고 한국어로 답합니다.",
            resume_context: &resume,
            repository_context: &repository,
            current_request: "현재 실패 원인을 분석해줘",
            response_cue: "짧고 근거 중심으로 답하고 action contract를 마지막에 기록합니다.",
        },
    )
    .unwrap();

    assert!(assembled.estimated_tokens <= assembled.input_limit_tokens);
    assert!(assembled.text.contains("필수 runtime 계약"));
    assert!(assembled
        .text
        .contains("<RESUME_CONTEXT trust=\"untrusted\">"));
    assert!(assembled
        .text
        .contains("<REPOSITORY_CONTEXT trust=\"untrusted\">"));
    assert!(assembled.text.ends_with(
        "<CURRENT_USER_REQUEST>\n현재 실패 원인을 분석해줘\n</CURRENT_USER_REQUEST>\n\n짧고 근거 중심으로 답하고 action contract를 마지막에 기록합니다."
    ));
}
