use crate::adapters::filesystem::layout as paths;
use crate::app::context_adapter::ContextPack;
use crate::app::extensions_adapter::skill;
use crate::runtime_core::patch::intent::has_any;

pub(super) fn available_context_labels(
    manifest: &skill::ResolvedSkillManifest,
    request: &str,
    context_pack: &ContextPack,
) -> Vec<&'static str> {
    let request_lower = request.to_ascii_lowercase();
    let has_pointer = !context_pack.source_pointers.is_empty();
    let has_test_signal =
        has_any(&request_lower, &["test", "pytest", "cargo test"]) || has_any(request, &["테스트"]);
    let has_test_output = has_any(
        &request_lower,
        &[
            "test result: failed",
            "assertion failed",
            "panicked at",
            "failed:",
            "failures:",
        ],
    ) || has_any(request, &["테스트 결과:", "실패 로그:", "검증 출력:"]);
    let has_error_output = has_any(
        &request_lower,
        &["error[", "error:", "panicked at", "traceback", "exception:"],
    ) || has_any(request, &["에러 로그:", "오류 출력:", "예외:"]);
    let project_root = paths::project_root();
    let has_package_manifest = ["Cargo.toml", "package.json", "pyproject.toml", "go.mod"]
        .iter()
        .any(|name| project_root.join(name).is_file());

    manifest
        .context_requirements()
        .iter()
        .copied()
        .filter(|requirement| match *requirement {
            "repo_root" => true,
            "acceptance_criteria" => !request.trim().is_empty(),
            "target_file" | "source_pointer" | "diff_or_files" => has_pointer,
            "test_output" => has_test_output,
            "error_output" => has_error_output,
            "test_context" => has_test_signal,
            "package_manifest" => has_package_manifest,
            "ontology_source" => context_pack.ontology_records_selected > 0,
            "runtime_state" => pointer_path_contains(context_pack, "state"),
            "operation_log" => {
                pointer_path_contains(context_pack, "log")
                    || pointer_path_contains(context_pack, "ledger")
            }
            "release_scope" => {
                has_any(&request_lower, &["release", "version"])
                    || has_any(request, &["릴리스", "버전"])
            }
            "test_results" => has_test_output,
            "model_manifest" | "model_source" => pointer_path_contains(context_pack, "model"),
            "benchmark_spec" => pointer_path_contains(context_pack, "benchmark"),
            "license_source" => pointer_path_contains(context_pack, "license"),
            "artifact_manifest" => pointer_path_contains(context_pack, "manifest"),
            _ => false,
        })
        .collect()
}

fn pointer_path_contains(context_pack: &ContextPack, needle: &str) -> bool {
    context_pack
        .source_pointers
        .iter()
        .any(|pointer| pointer.path.to_ascii_lowercase().contains(needle))
}
