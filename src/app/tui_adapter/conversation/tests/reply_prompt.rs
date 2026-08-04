use super::super::reply::prompt::assemble_plain_prompt_with_runtime_evidence;

#[test]
fn plain_answer_prompt_keeps_current_tool_evidence_ahead_of_large_attachment() {
    let prompt = assemble_plain_prompt_with_runtime_evidence(
        "Cargo.toml을 확인해줘",
        &format!(
            "Cargo.toml을 확인해줘\n{}",
            "oversized-attachment ".repeat(20_000)
        ),
        "RUNTIME_LOCAL_OBSERVATIONS version-0.55.1",
        &[],
        &[],
        4_096,
    )
    .unwrap();

    assert!(prompt.text.contains("CURRENT_TURN_EVIDENCE"));
    assert!(prompt.text.contains("version-0.55.1"));
    assert!(prompt.estimated_tokens <= prompt.input_limit_tokens);
}
