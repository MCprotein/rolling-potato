fn assert_application_model_owners() {
    let model_adapter_path = "src/app/inference_adapter/model.rs";
    let model_reports_path = "src/app/inference_adapter/model/reports.rs";
    let model_evaluation_path = "src/app/inference_adapter/model/evaluation.rs";
    let model_promotion_path = "src/app/inference_adapter/model/promotion.rs";
    let model_artifact_maintenance_path = "src/app/inference_adapter/model/artifact_maintenance.rs";
    let model_evidence_path = "src/app/inference_adapter/model/evidence.rs";
    let model_registry_path = "src/app/inference_adapter/model/registry.rs";
    let model_registry_vision_path = "src/app/inference_adapter/model/registry/vision.rs";
    let model_registry_vision_preparation_path =
        "src/app/inference_adapter/model/registry/vision/preparation.rs";
    let model_registry_vision_preparation_tests_path =
        "src/app/inference_adapter/model/registry/vision/preparation/tests.rs";
    let model_default_selection_path =
        "src/app/inference_adapter/model/registry/default_selection.rs";
    let model_setup_path = "src/app/inference_adapter/model/setup.rs";
    let model_setup_catalog_path = "src/app/inference_adapter/model/setup/catalog.rs";
    let model_runtime_spec_path = "src/app/inference_adapter/model/setup/runtime_spec.rs";
    let model_setup_tests_path = "src/app/inference_adapter/model/setup/tests.rs";
    let model_promotion_tests_path = "src/app/inference_adapter/model/promotion_tests.rs";
    let model_tests_path = "src/app/inference_adapter/model/tests.rs";
    assert!(Path::new(model_reports_path).is_file());
    assert!(Path::new(model_evaluation_path).is_file());
    assert!(Path::new(model_promotion_path).is_file());
    assert!(Path::new(model_artifact_maintenance_path).is_file());
    assert!(Path::new(model_evidence_path).is_file());
    assert!(Path::new(model_registry_path).is_file());
    assert!(Path::new(model_registry_vision_path).is_file());
    assert!(Path::new(model_registry_vision_preparation_path).is_file());
    assert!(Path::new(model_registry_vision_preparation_tests_path).is_file());
    assert!(Path::new(model_default_selection_path).is_file());
    assert!(Path::new(model_setup_path).is_file());
    assert!(Path::new(model_setup_catalog_path).is_file());
    assert!(Path::new(model_runtime_spec_path).is_file());
    assert!(Path::new(model_setup_tests_path).is_file());
    assert!(Path::new(model_promotion_tests_path).is_file());
    assert!(Path::new(model_tests_path).is_file());
    let model_adapter = fs::read_to_string(model_adapter_path).unwrap();
    let model_reports = fs::read_to_string(model_reports_path).unwrap();
    let model_evaluation = fs::read_to_string(model_evaluation_path).unwrap();
    let model_promotion = fs::read_to_string(model_promotion_path).unwrap();
    let model_artifact_maintenance = fs::read_to_string(model_artifact_maintenance_path).unwrap();
    let model_evidence = fs::read_to_string(model_evidence_path).unwrap();
    let model_registry = fs::read_to_string(model_registry_path).unwrap();
    let model_registry_vision = fs::read_to_string(model_registry_vision_path).unwrap();
    let model_registry_vision_preparation =
        fs::read_to_string(model_registry_vision_preparation_path).unwrap();
    let model_registry_vision_preparation_tests =
        fs::read_to_string(model_registry_vision_preparation_tests_path).unwrap();
    let model_default_selection = fs::read_to_string(model_default_selection_path).unwrap();
    let model_setup = fs::read_to_string(model_setup_path).unwrap();
    let model_setup_catalog = fs::read_to_string(model_setup_catalog_path).unwrap();
    let model_runtime_spec = fs::read_to_string(model_runtime_spec_path).unwrap();
    let model_promotion_tests = fs::read_to_string(model_promotion_tests_path).unwrap();
    let model_tests = fs::read_to_string(model_tests_path).unwrap();
    assert!(
        model_adapter.contains("#[path = \"model/promotion_tests.rs\"]"),
        "model adapter does not register its promotion regression-test owner"
    );
    assert!(
        model_adapter.contains("#[path = \"model/tests.rs\"]"),
        "model adapter does not register its regression-test owner"
    );
    assert!(
        model_adapter.lines().any(|line| line == "mod evidence;"),
        "model adapter does not register its local evidence owner"
    );
    for owner in [
        "mod artifact_maintenance;",
        "mod evaluation;",
        "mod promotion;",
        "mod reports;",
    ] {
        assert!(
            model_adapter.lines().any(|line| line == owner),
            "model adapter does not register owner: {owner}"
        );
    }
    assert!(
        model_registry.lines().any(|line| line == "mod vision;"),
        "model registry does not register its vision owner"
    );
    assert!(
        model_registry_vision
            .lines()
            .any(|line| line == "mod preparation;"),
        "model registry vision owner does not register its lazy preparation owner"
    );
    for responsibility in [
        "pub(crate) struct VerifiedVisionProjector",
        "pub(crate) fn verified_vision_projector(",
        "pub(super) fn local_registry_vision(",
    ] {
        assert!(
            model_registry_vision.contains(responsibility),
            "model registry vision owner is missing: {responsibility}"
        );
        assert!(
            !model_registry.contains(responsibility),
            "model registry facade still owns vision verification: {responsibility}"
        );
    }
    assert!(
        model_registry_vision_preparation.contains("pub(crate) fn prepare_bound_vision_projector("),
        "model registry vision preparation owner is missing its projector preparation"
    );
    for forbidden in [
        "fetch_managed_projector_artifact",
        "require_declared_projector",
        "vision_projector_artifact_path",
    ] {
        assert!(
            !model_setup.contains(forbidden),
            "base model setup must not own eager projector preparation: {forbidden}"
        );
    }
    assert!(
        !model_adapter.contains("fetch_managed_projector_artifact"),
        "model evaluation fetch must not eagerly download an optional projector"
    );
    for regression in [
        "model_upgrade_compatibility_image_use_migrates_v1_binding_and_preserves_state",
        "model_upgrade_compatibility_preparation_failure_preserves_registry_default_and_backend",
    ] {
        assert!(
            model_registry_vision_preparation_tests.contains(regression),
            "projector preparation regression owner is missing: {regression}"
        );
    }
    for responsibility in [
        "pub(super) fn local_benchmark_status(",
        "pub(super) fn local_promotion_readiness(",
        "pub(super) fn promotion_benchmark_run(",
        "pub(super) fn promotion_benchmark_evidence(",
        "pub(super) fn backend_smoke_evidence(",
        "pub(super) fn persist_promotion_evidence(",
        "pub(super) fn read_promotion_evidence_file(",
    ] {
        assert!(
            model_evidence.contains(responsibility),
            "model local evidence owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns local evidence collection: {responsibility}"
        );
    }
    assert!(
        model_adapter.lines().any(|line| line == "mod registry;"),
        "model adapter does not register its registry owner"
    );
    assert!(
        model_adapter.lines().any(|line| line == "mod setup;"),
        "model adapter does not register its setup owner"
    );
    assert!(model_setup.lines().any(|line| line == "mod catalog;"));
    assert!(model_setup.lines().any(|line| line == "mod runtime_spec;"));
    assert!(model_setup.lines().any(|line| line == "mod tests;"));
    assert!(model_setup_catalog.contains("pub(super) fn setup_options("));
    for responsibility in [
        "pub(crate) struct PreparedSetupModel",
        "pub(crate) fn setup_options(",
        "pub(crate) fn prepare_setup_model(",
        "pub(crate) fn activate_setup_model(",
    ] {
        assert!(
            model_setup.contains(responsibility),
            "model setup owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns interactive setup: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) struct ConfiguredRuntimeSpec",
        "pub(crate) fn configured_runtime_spec(",
        "pub(crate) fn configured_vision_runtime_spec(",
    ] {
        assert!(
            model_runtime_spec.contains(responsibility),
            "configured runtime specification owner is missing: {responsibility}"
        );
        assert!(
            !model_setup.contains(responsibility),
            "interactive setup still owns configured runtime validation: {responsibility}"
        );
    }
    assert!(
        model_registry
            .lines()
            .any(|line| line == "mod default_selection;"),
        "model registry does not register its default-selection owner"
    );
    for responsibility in [
        "pub(crate) struct DefaultSelectionSnapshot",
        "pub(crate) fn snapshot_default_selection(",
        "pub(crate) fn restore_default_selection(",
    ] {
        assert!(
            model_default_selection.contains(responsibility),
            "default-selection owner is missing: {responsibility}"
        );
        assert!(
            !model_registry.contains(responsibility),
            "model registry still owns default-selection rollback: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn registry_report(",
        "pub fn default_report(",
        "pub fn set_default_report(",
        "pub fn default_artifact_path(",
        "pub fn install_candidate(",
        "fn validated_registry_entry(",
        "pub(super) fn registry_entry_json(",
    ] {
        assert!(
            model_registry.contains(responsibility),
            "model registry owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns registry behavior: {responsibility}"
        );
    }
    for responsibility in [
        "fn manifest_validation_blocks_unverified_artifact_candidate(",
        "fn cleanup_failed_dry_run_lists_app_managed_paths(",
    ] {
        assert!(
            model_tests.contains(responsibility),
            "model regression owner is missing: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model adapter still owns regression test: {responsibility}"
        );
    }
    for responsibility in [
        "fn promotion_evidence_validation_accepts_measured_local_benchmark(",
        "fn registry_promotion_binding_rejects_backend_and_benchmark_drift(",
    ] {
        assert!(
            model_promotion_tests.contains(responsibility),
            "model promotion regression owner is missing: {responsibility}"
        );
        assert!(
            !model_tests.contains(responsibility),
            "general model regression owner still owns promotion test: {responsibility}"
        );
    }
    for (owner, responsibility) in [
        (&model_reports, "pub fn candidate_summary("),
        (&model_reports, "pub fn list_report("),
        (&model_reports, "pub fn manifest_report("),
        (&model_reports, "pub fn inspect_report("),
        (&model_reports, "pub fn download_plan_report("),
        (&model_reports, "pub fn benchmark_plan_report("),
        (&model_evaluation, "pub fn eval_plan_report("),
        (
            &model_evaluation,
            "pub fn fetch_candidate_for_evaluation_report(",
        ),
        (
            &model_evaluation,
            "pub(crate) fn fetch_candidate_for_evaluation(",
        ),
        (&model_promotion, "pub fn promote_candidate_report("),
        (&model_artifact_maintenance, "pub fn verify_file_report("),
        (&model_artifact_maintenance, "pub fn cleanup_failed_report("),
    ] {
        assert!(
            owner.contains(responsibility),
            "model owner is missing responsibility: {responsibility}"
        );
        assert!(
            !model_adapter.contains(responsibility),
            "model facade still owns responsibility: {responsibility}"
        );
    }
    assert!(
        model_adapter.lines().count() < 100,
        "model adapter regrew beyond its facade boundary"
    );
    assert!(model_reports.lines().count() < 350);
    assert!(model_evaluation.lines().count() < 200);
    assert!(model_promotion.lines().count() < 175);
    assert!(model_artifact_maintenance.lines().count() < 125);
    assert!(
        model_evidence.lines().count() < 250,
        "model local evidence module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry.lines().count() < 350,
        "model registry module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision.lines().count() < 125,
        "model registry vision module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision_preparation.lines().count() < 125,
        "model registry vision preparation module regrew beyond its ownership boundary"
    );
    assert!(
        model_registry_vision_preparation_tests.lines().count() < 225,
        "model registry vision preparation tests regrew beyond their ownership boundary"
    );
    assert!(model_default_selection.lines().count() < 75);
    assert!(model_setup.lines().count() < 100);
    assert!(model_runtime_spec.lines().count() < 125);
    assert!(
        model_promotion_tests.lines().count() < 425,
        "model promotion regression module regrew beyond its ownership boundary"
    );
    assert!(
        model_tests.lines().count() < 550,
        "model regression module regrew beyond its ownership boundary"
    );
}
