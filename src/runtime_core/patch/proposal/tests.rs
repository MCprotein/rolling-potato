use std::path::Path;

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
