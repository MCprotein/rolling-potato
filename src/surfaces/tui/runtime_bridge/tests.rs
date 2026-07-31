use super::*;

#[test]
fn read_budget_and_placeholder_keep_bounded_explicit_state() {
    assert_eq!(
        TuiReadBudget::bounded(0, usize::MAX),
        TuiReadBudget {
            max_items: 1,
            max_chars: TUI_MAX_CHARS,
        }
    );

    let page = TuiReadPage::conversation_placeholder();
    assert_eq!(page.title, "conversation");
    assert!(page.lines.is_empty());
    assert_eq!(page.freshness, TuiFreshness::Unavailable);
    assert_eq!(page.continuation, TuiReadContinuation::Unavailable);
    assert_eq!(page.authority, TuiReadAuthority::default());
}

#[test]
fn model_labels_distinguish_model_and_lazy_projector_cache_state() {
    let mut option = TuiModelOption {
        id: "model".to_string(),
        display_name: "Model".to_string(),
        quantization: "Q4".to_string(),
        download_bytes: 3 * 1024 * 1024 * 1024,
        model_cached: false,
        vision_projector_bytes: Some(512 * 1024 * 1024),
        vision_projector_cached: false,
        context_length: Some(32_768),
        ram: "8 GiB".to_string(),
        license: "test".to_string(),
        note: String::new(),
        current: false,
        evaluation_recommended: false,
        readiness: TuiModelReadiness::EvaluationOnly,
    };

    assert_eq!(option.model_artifact_label(), "download 3.0 GiB");
    assert_eq!(
        option.vision_artifact_label(),
        "on-demand · 첫 이미지에서 projector 0.5 GiB 자동 준비"
    );

    option.model_cached = true;
    option.vision_projector_cached = true;
    assert_eq!(
        option.model_artifact_label(),
        "local cache · 적용 시 SHA-256 검증"
    );
    assert_eq!(
        option.vision_artifact_label(),
        "on-demand · projector cache 준비됨"
    );
}

#[test]
fn selection_lease_requires_the_exact_observed_revision_and_workflow() {
    let observation = SelectionObservation {
        project_id: "project".to_string(),
        session_id: "session".to_string(),
        current_revision: 7,
        current_hash: "state-hash".to_string(),
        active_workflow: Some(ObservedWorkflow {
            workflow_id: "workflow".to_string(),
            revision: 3,
            hash: "workflow-hash".to_string(),
        }),
    };
    let lease = observation.lease_for("workflow");

    assert!(lease_matches_active_workflow(
        &lease,
        "workflow",
        &observation
    ));
    assert!(lease_matches_terminal_selection(
        &lease,
        "workflow",
        &observation
    ));

    let mut changed = observation.clone();
    changed.current_revision += 1;
    assert!(!lease_matches_active_workflow(&lease, "workflow", &changed));
    assert!(!lease_matches_terminal_selection(
        &lease, "workflow", &changed
    ));
}

#[test]
fn intent_ids_are_unique_and_one_shot_secret_rejects_empty_values() {
    let first = new_tui_intent_id();
    let second = new_tui_intent_id();
    assert!(first.starts_with("intent-tui-"));
    assert_ne!(first, second);

    assert!(OneShotSecret::new(String::new()).is_err());
    let value = OneShotSecret::new("secret".to_string())
        .expect("non-empty secret")
        .expose(str::to_string);
    assert_eq!(value, "secret");
}
