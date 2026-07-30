const PRODUCT_GENERATION_CALLERS: &[&str] = &[
    "src/app/tui_adapter/conversation/reply.rs",
    "src/app/tui_adapter/conversation/decision.rs",
    "src/app/tui_adapter/attachment.rs",
    "src/app/tui_adapter/attachment/compose.rs",
    "src/app/inference_adapter/answer.rs",
    "src/app/web_search_adapter/page_tools/find.rs",
    "src/app/web_search_adapter/page_tools/open.rs",
    "src/app/web_search_adapter/research/types.rs",
    "src/app/web_search_adapter/research_flow.rs",
    "src/app/intent_adapter/execution/model_turn.rs",
    "src/app/context_adapter/compaction.rs",
];

#[test]
fn central_generation_owners_are_registered_and_bounded() {
    let policy_facade_path = "src/runtime_core/inference/mod.rs";
    let policy_path = "src/runtime_core/inference/generation_policy/mod.rs";
    let policy_tests_path = "src/runtime_core/inference/generation_policy/tests.rs";
    let backend_facade_path = "src/app/inference_adapter/backend.rs";
    let gateway_path = "src/app/inference_adapter/backend/generation_gateway.rs";

    for path in [policy_path, policy_tests_path, gateway_path] {
        assert!(
            Path::new(path).is_file(),
            "model-aware generation owner is missing: {path}"
        );
    }

    let policy_facade = fs::read_to_string(policy_facade_path).unwrap();
    let policy = fs::read_to_string(policy_path).unwrap();
    let policy_tests = fs::read_to_string(policy_tests_path).unwrap();
    let backend_facade = fs::read_to_string(backend_facade_path).unwrap();
    let gateway = fs::read_to_string(gateway_path).unwrap();

    assert!(
        policy_facade
            .lines()
            .any(|line| line.trim() == "pub(crate) mod generation_policy;"),
        "runtime inference facade must register the central generation policy"
    );
    assert!(
        backend_facade
            .lines()
            .any(|line| line.trim() == "mod generation_gateway;"),
        "backend adapter must register the model-aware generation gateway"
    );
    assert!(
        policy
            .lines()
            .any(|line| line.trim() == "mod tests;"),
        "central generation policy must retain focused regression tests"
    );

    assert!(
        policy.lines().count() < 650,
        "generation policy exceeded its pure-calculation ownership boundary"
    );
    assert!(
        policy_tests.lines().count() < 600,
        "generation policy regression owner needs responsibility-based splitting"
    );
    assert!(
        gateway.lines().count() < 400,
        "generation gateway exceeded its runtime-binding ownership boundary"
    );
}

#[test]
fn product_generation_callers_do_not_own_numeric_completion_caps() {
    let mut violations = Vec::new();

    for path in PRODUCT_GENERATION_CALLERS {
        let source =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
        for (index, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default().trim();
            if declares_numeric_generation_constant(code)
                || assigns_numeric_max_tokens(code)
                || code.contains("GenerationTokenRequest::ExplicitBound(")
            {
                violations.push(format!("{path}:{}: {code}", index + 1));
            }
        }

        for legacy_api in [
            "backend::chat_once(",
            "backend::chat_once_bounded(",
            "backend::chat_once_bounded_with_cancel(",
            "backend::chat_once_with_input(",
        ] {
            if source.contains(legacy_api) {
                violations.push(format!("{path}: legacy raw-cap API `{legacy_api}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "product generation must request an intent from the central generation policy/gateway; \
         benchmark, collaboration/governance, and low-level request serialization are the only \
         raw bounded paths:\n{}",
        violations.join("\n")
    );
}

#[test]
fn answer_and_web_research_apis_do_not_export_raw_token_budgets() {
    let answer = fs::read_to_string("src/app/inference_adapter/answer.rs").unwrap();
    let research_types =
        fs::read_to_string("src/app/web_search_adapter/research/types.rs").unwrap();
    let research_flow = fs::read_to_string("src/app/web_search_adapter/research_flow.rs").unwrap();
    let attachment = fs::read_to_string("src/app/tui_adapter/attachment.rs").unwrap();

    assert!(
        !answer.contains("max_tokens: u32"),
        "visible-answer APIs must accept a generation intent, not a raw token count"
    );
    assert!(
        !research_types.contains("final_answer_tokens")
            && !research_flow.contains("final_answer_tokens"),
        "web research answer length belongs to the central generation policy, not its search budget"
    );
    assert!(
        !attachment.contains("RESPONSE_RESERVE_TOKENS")
            && !attachment.contains("RUNTIME_PROMPT_RESERVE_TOKENS"),
        "attachment prompt space must use the same model-window policy as conversation prompts"
    );
}

#[test]
fn model_request_behavior_is_not_guessed_from_names_or_global_sampling() {
    let runtime_profile =
        fs::read_to_string("src/app/inference_adapter/backend/chat/runtime_profile.rs").unwrap();
    let request = fs::read_to_string("src/adapters/llama_cpp/backend/request.rs").unwrap();
    let report =
        fs::read_to_string("src/app/inference_adapter/backend/chat/report.rs").unwrap();
    let chat = fs::read_to_string("src/app/inference_adapter/backend/chat.rs").unwrap();
    let startup =
        fs::read_to_string("src/app/inference_adapter/backend/sidecar/startup.rs").unwrap();
    let policy =
        fs::read_to_string("src/runtime_core/inference/generation_policy/mod.rs").unwrap();

    for (path, source) in [
        ("runtime_profile.rs", runtime_profile.as_str()),
        ("request.rs", request.as_str()),
        ("report.rs", report.as_str()),
    ] {
        assert!(
            !source.contains("starts_with(\"qwen")
                && !source.contains("starts_with(\"gemma")
                && !source.contains("contains(\"qwen")
                && !source.contains("contains(\"gemma"),
            "{path} must resolve model behavior by exact artifact metadata, not names"
        );
    }
    assert!(
        !chat.contains("CHAT_SAMPLING"),
        "chat adapter must not own a global sampling profile"
    );
    assert!(
        !startup.contains("sampling=temperature-"),
        "sidecar startup cannot claim a generation profile before a chat request"
    );
    assert!(
        !policy.contains("uncalibrated_throughput_tokens_per_second")
            && !policy.contains("ThroughputInput"),
        "deadline capacity must require managed artifact/backend throughput evidence"
    );
}

#[test]
fn setup_recommendations_are_manifest_driven_not_model_id_branches() {
    let catalog =
        fs::read_to_string("src/app/inference_adapter/model/setup/catalog.rs").unwrap();

    assert!(
        !catalog.contains("candidate.id ==")
            && !catalog.contains("model_id ==")
            && !catalog.contains("model_id: &str"),
        "setup recommendation and adoption notes must come from exact manifest metadata"
    );
}

#[test]
fn visible_answer_contracts_do_not_own_fixed_product_length_caps() {
    for path in [
        "src/runtime_core/agent.rs",
        "src/app/inference_adapter/answer.rs",
        "src/app/web_search_adapter/answer_contract.rs",
    ] {
        let source =
            fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
        assert!(
            !source.contains("MAX_ANSWER_CHARS")
                && !source.contains("MAX_REPAIR_INPUT_CHARS"),
            "{path} must use backend protocol limits or the active model-window policy"
        );
    }
}

#[test]
fn numeric_completion_constants_exist_only_in_explicit_governed_contracts() {
    let allowed = BTreeSet::from([
        (
            "src/runtime_core/collaboration/subagent/types.rs",
            "DEFAULT_MAX_TOKENS",
        ),
        (
            "src/runtime_core/collaboration/subagent/types.rs",
            "MAX_MAX_TOKENS",
        ),
        (
            "src/runtime_core/inference/benchmark.rs",
            "ADOPTION_MAX_TOKENS",
        ),
        (
            "src/runtime_core/inference/resource.rs",
            "DEGRADED_CHAT_MAX_TOKENS",
        ),
    ]);
    let mut rust_files = BTreeSet::new();
    collect_recursive(
        Path::new("src"),
        &BTreeSet::from(["rs".to_string()]),
        &mut rust_files,
    );
    let mut violations = Vec::new();

    for path in rust_files {
        let source = fs::read_to_string(&path).unwrap();
        for (index, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default().trim();
            let Some(name) = numeric_generation_constant_name(code) else {
                continue;
            };
            if !allowed.contains(&(path.as_str(), name)) {
                violations.push(format!("{path}:{}: {code}", index + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "raw numeric completion constants require an explicit benchmark, governance, resource, or protocol owner:\n{}",
        violations.join("\n")
    );
}

fn declares_numeric_generation_constant(code: &str) -> bool {
    numeric_generation_constant_name(code).is_some()
}

fn numeric_generation_constant_name(code: &str) -> Option<&str> {
    let declaration = code
        .strip_prefix("const ")
        .or_else(|| code.split_once(" const ").map(|(_, suffix)| suffix))?;
    let name = declaration.split([':', '=']).next()?.trim();
    if !name.ends_with("MAX_TOKENS") {
        return None;
    }
    code.split_once('=')
        .filter(|(_, value)| starts_with_numeric_literal(value))
        .map(|_| name)
}

fn assigns_numeric_max_tokens(code: &str) -> bool {
    let Some((_, suffix)) = code.split_once("max_tokens") else {
        return false;
    };
    let suffix = suffix.trim_start();
    let Some(value) = suffix
        .strip_prefix(':')
        .or_else(|| suffix.strip_prefix('='))
    else {
        return false;
    };
    starts_with_numeric_literal(value)
}

fn starts_with_numeric_literal(value: &str) -> bool {
    let value = value.trim_start();
    let value = value.strip_prefix("Some(").unwrap_or(value).trim_start();
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
}

#[test]
fn numeric_generation_cap_detector_ignores_non_generation_limits() {
    for unrelated in [
        "const MAX_STORAGE_BYTES: usize = 1024;",
        "const CONTEXT_WINDOW_TOKENS: u32 = 131_072;",
        "const MAX_REPAIR_INPUT_CHARS: usize = 8 * 1024;",
        "let max_tokens = policy.summary_output_budget_tokens;",
    ] {
        assert!(!declares_numeric_generation_constant(unrelated));
        assert!(!assigns_numeric_max_tokens(unrelated));
    }
}

#[test]
fn numeric_generation_cap_detector_rejects_literal_caps() {
    for raw_cap in [
        "const ANSWER_MAX_TOKENS: u32 = 512;",
        "pub(super) const REPAIR_MAX_TOKENS: usize = 1_024;",
        "max_tokens: 768,",
        "max_tokens = Some(384);",
    ] {
        assert!(
            declares_numeric_generation_constant(raw_cap)
                || assigns_numeric_max_tokens(raw_cap),
            "raw generation cap escaped the architecture guard: {raw_cap}"
        );
    }
}
