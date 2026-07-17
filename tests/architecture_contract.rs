#![allow(dead_code)]

mod foundation {
    pub(crate) mod error {
        #[derive(Debug)]
        pub(crate) struct AppError {
            pub(crate) message: String,
        }

        impl AppError {
            pub(crate) fn blocked(message: String) -> Self {
                Self { message }
            }
        }
    }
}

#[path = "../src/foundation/serialization.rs"]
mod strict_json;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use strict_json::{Object, Value};

const MAP_PATH: &str = "docs/architecture-migration-map.json";
const ARCHITECTURE_ROOTS: [&str; 5] = [
    "composition",
    "surfaces",
    "runtime_core",
    "adapters",
    "foundation",
];

fn load_map() -> Value {
    let input = fs::read_to_string(MAP_PATH).expect("migration map must be readable");
    strict_json::parse_value(&input, MAP_PATH).expect("migration map must be valid strict JSON")
}

fn as_object<'a>(value: &'a Value, context: &str) -> &'a Object {
    let Value::Object(object) = value else {
        panic!("{context} must be an object");
    };
    object
}

fn field<'a>(object: &'a Object, key: &str, context: &str) -> &'a Value {
    object
        .get(key)
        .unwrap_or_else(|| panic!("{context} is missing {key}"))
}

fn field_object<'a>(object: &'a Object, key: &str, context: &str) -> &'a Object {
    as_object(field(object, key, context), &format!("{context}.{key}"))
}

fn field_array<'a>(object: &'a Object, key: &str, context: &str) -> &'a [Value] {
    let Value::Array(values) = field(object, key, context) else {
        panic!("{context}.{key} must be an array");
    };
    values
}

fn field_string<'a>(object: &'a Object, key: &str, context: &str) -> &'a str {
    let Value::String(value) = field(object, key, context) else {
        panic!("{context}.{key} must be a string");
    };
    value
}

fn field_bool(object: &Object, key: &str, context: &str) -> bool {
    let Value::Bool(value) = field(object, key, context) else {
        panic!("{context}.{key} must be a boolean");
    };
    *value
}

fn string_array(values: &[Value], context: &str) -> Vec<String> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let Value::String(value) = value else {
                panic!("{context}[{index}] must be a string");
            };
            value.clone()
        })
        .collect()
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn collect_recursive(root: &Path, extensions: &BTreeSet<String>, output: &mut BTreeSet<String>) {
    let mut entries = fs::read_dir(root)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", root.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|err| panic!("cannot enumerate {}: {err}", root.display()));
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .unwrap_or_else(|err| panic!("cannot inspect {}: {err}", path.display()));
        if file_type.is_dir() {
            collect_recursive(&path, extensions, output);
        } else if file_type.is_file()
            && (extensions.is_empty()
                || path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| extensions.contains(value)))
        {
            output.insert(normalized(&path));
        }
    }
}

fn governed_paths(root: &Object) -> BTreeSet<String> {
    let governed = field_object(root, "governed", "map");
    let mut paths = BTreeSet::new();

    for (index, value) in field_array(governed, "recursive", "map.governed")
        .iter()
        .enumerate()
    {
        let context = format!("map.governed.recursive[{index}]");
        let rule = as_object(value, &context);
        let root = field_string(rule, "root", &context);
        let extensions = string_array(field_array(rule, "extensions", &context), &context)
            .into_iter()
            .collect();
        collect_recursive(Path::new(root), &extensions, &mut paths);
    }

    for path in string_array(
        field_array(governed, "root_files", "map.governed"),
        "map.governed.root_files",
    ) {
        assert!(
            Path::new(&path).is_file(),
            "governed root file is missing: {path}"
        );
        paths.insert(path);
    }
    paths
}

fn release_patch(release: &str, context: &str) -> u16 {
    let patch = release
        .strip_prefix("0.37.")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("{context} must use a v0.37.x release: {release}"));
    assert!(patch >= 1, "{context} cannot target v0.37.0: {release}");
    patch
}

fn target_is_exact(target: &str) -> bool {
    !target.is_empty()
        && !target.contains('*')
        && !target.contains(',')
        && !target.contains(" or ")
        && !target.contains("..")
        && !target.contains('[')
        && !target.contains(']')
}

fn lifecycle_violation(
    state: &str,
    scheduled_patch: u16,
    current_patch: u16,
    train_completion: bool,
    expiry_patch: Option<u16>,
) -> Option<&'static str> {
    if train_completion && state != "complete" {
        return Some("train completion requires every slice to be complete");
    }
    if state == "exception" && expiry_patch.is_some_and(|expiry| expiry < current_patch) {
        return Some("exception expired before the current release");
    }
    if scheduled_patch <= current_patch
        && matches!(state, "planned" | "migrating" | "compatibility-facade")
    {
        return Some("scheduled release has an unfinished migration slice");
    }
    None
}

fn logical_proof_ids(root: &Object) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (index, value) in field_array(root, "logical_proofs", "map")
        .iter()
        .enumerate()
    {
        let context = format!("map.logical_proofs[{index}]");
        let proof = as_object(value, &context);
        let id = field_string(proof, "id", &context).to_owned();
        let entrypoint = field_string(proof, "entrypoint", &context);
        assert!(!id.trim().is_empty(), "{context}.id is empty");
        assert!(
            !entrypoint.trim().is_empty(),
            "{context}.entrypoint is empty"
        );
        assert!(ids.insert(id.clone()), "duplicate logical proof id: {id}");
    }
    ids
}

fn responsibility_inventory(root: &Object) -> BTreeMap<String, BTreeSet<String>> {
    let mut inventory = BTreeMap::new();
    for (index, value) in field_array(root, "responsibility_inventory", "map")
        .iter()
        .enumerate()
    {
        let context = format!("map.responsibility_inventory[{index}]");
        let record = as_object(value, &context);
        let path = field_string(record, "path", &context).to_owned();
        let responsibilities =
            string_array(field_array(record, "responsibilities", &context), &context)
                .into_iter()
                .collect::<BTreeSet<_>>();
        assert!(!responsibilities.is_empty(), "{context} is empty");
        assert!(
            inventory.insert(path.clone(), responsibilities).is_none(),
            "duplicate responsibility inventory: {path}"
        );
    }
    inventory
}

fn validate_slice(
    slice: &Object,
    context: &str,
    current_patch: u16,
    train_completion: bool,
    proof_ids: &BTreeSet<String>,
) -> (String, String) {
    const BASE_KEYS: [&str; 5] = ["responsibility", "target", "release", "state", "evidence"];
    const EXCEPTION_KEYS: [&str; 3] = ["owner", "rationale", "expiry_release"];

    let responsibility = field_string(slice, "responsibility", context);
    let target = field_string(slice, "target", context);
    let release = field_string(slice, "release", context);
    let state = field_string(slice, "state", context);
    let evidence = field_array(slice, "evidence", context);

    assert!(
        !responsibility.trim().is_empty(),
        "{context} responsibility is empty"
    );
    assert!(
        target_is_exact(target),
        "{context} target is ambiguous: {target}"
    );
    let scheduled_patch = release_patch(release, context);
    assert!(
        [
            "planned",
            "migrating",
            "compatibility-facade",
            "complete",
            "exception",
        ]
        .contains(&state),
        "{context} has invalid state: {state}"
    );
    for (index, value) in evidence.iter().enumerate() {
        let Value::String(value) = value else {
            panic!("{context}.evidence[{index}] must be a string");
        };
        assert!(
            !value.trim().is_empty(),
            "{context}.evidence[{index}] is empty"
        );
        let proof_id = value.strip_prefix("proof:");
        assert!(
            Path::new(value).is_file()
                || proof_id.is_some_and(|proof_id| proof_ids.contains(proof_id)),
            "{context}.evidence[{index}] is neither an existing proof path nor a declared logical proof id: {value}"
        );
    }
    if state == "complete" {
        assert!(
            !evidence.is_empty(),
            "{context} complete slice needs evidence"
        );
    }

    let allowed_keys = if state == "exception" {
        BASE_KEYS
            .iter()
            .chain(EXCEPTION_KEYS.iter())
            .copied()
            .collect::<BTreeSet<_>>()
    } else {
        BASE_KEYS.into_iter().collect::<BTreeSet<_>>()
    };
    for key in slice.keys() {
        assert!(
            allowed_keys.contains(key.as_str()),
            "{context} has unknown key: {key}"
        );
    }

    let expiry_patch = if state == "exception" {
        assert!(!field_string(slice, "owner", context).trim().is_empty());
        assert!(!field_string(slice, "rationale", context).trim().is_empty());
        Some(release_patch(
            field_string(slice, "expiry_release", context),
            context,
        ))
    } else {
        None
    };
    if let Some(reason) = lifecycle_violation(
        state,
        scheduled_patch,
        current_patch,
        train_completion,
        expiry_patch,
    ) {
        panic!("{context} violates migration lifecycle: {reason}");
    }

    (responsibility.to_owned(), target.to_owned())
}

#[test]
fn migration_map_recursively_covers_every_governed_file_and_exact_slice() {
    let map = load_map();
    let root = as_object(&map, "map");
    assert_eq!(field_string(root, "train", "map"), "0.37.x");
    let current_patch = release_patch(field_string(root, "current_release", "map"), "map");
    let train_completion = field_bool(root, "train_completion", "map");
    let proof_ids = logical_proof_ids(root);
    let inventory = responsibility_inventory(root);
    let Value::Number(schema_version) = field(root, "schema_version", "map") else {
        panic!("map.schema_version must be an unsigned integer");
    };
    assert_eq!(*schema_version, 1);

    let expected = governed_paths(root);
    let mut records = BTreeMap::new();
    for (record_index, value) in field_array(root, "files", "map").iter().enumerate() {
        let context = format!("map.files[{record_index}]");
        let record = as_object(value, &context);
        let path = field_string(record, "path", &context).to_owned();
        assert!(
            records.insert(path.clone(), record_index).is_none(),
            "duplicate file record: {path}"
        );
        let slices = field_array(record, "slices", &context);
        assert!(
            !slices.is_empty(),
            "{context} must contain at least one slice"
        );
        let mut responsibilities = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for (slice_index, value) in slices.iter().enumerate() {
            let slice_context = format!("{context}.slices[{slice_index}]");
            let (responsibility, target) = validate_slice(
                as_object(value, &slice_context),
                &slice_context,
                current_patch,
                train_completion,
                &proof_ids,
            );
            assert!(
                responsibilities.insert(responsibility.clone()),
                "{context} repeats responsibility: {responsibility}"
            );
            assert!(
                targets.insert(target.clone()),
                "{context} repeats target: {target}"
            );
        }
        assert_eq!(
            inventory.get(&path),
            Some(&responsibilities),
            "{context} slices do not exactly and exclusively cover the separate responsibility inventory"
        );
    }

    let actual = records.keys().cloned().collect::<BTreeSet<_>>();
    let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
    let extra = actual.difference(&expected).cloned().collect::<Vec<_>>();
    assert!(missing.is_empty(), "unmapped governed files: {missing:#?}");
    assert!(extra.is_empty(), "stale migration records: {extra:#?}");
    assert_eq!(
        inventory.keys().cloned().collect::<BTreeSet<_>>(),
        actual,
        "responsibility inventory and file records must cover the same paths"
    );

    for invalid in [
        "",
        "src/a.rs or src/b.rs",
        "src/*.rs",
        "src/a.rs,src/b.rs",
        "src/a..b.rs",
    ] {
        assert!(
            !target_is_exact(invalid),
            "ambiguous target was accepted: {invalid}"
        );
    }
}

#[test]
fn completion_gate_rejects_expired_exceptions_and_incomplete_states() {
    assert_eq!(
        lifecycle_violation("exception", 8, 8, false, Some(7)),
        Some("exception expired before the current release")
    );
    assert_eq!(lifecycle_violation("exception", 8, 8, false, Some(8)), None);
    assert_eq!(
        lifecycle_violation("planned", 2, 2, false, None),
        Some("scheduled release has an unfinished migration slice")
    );
    assert_eq!(lifecycle_violation("planned", 3, 2, false, None), None);
    for state in ["planned", "migrating", "compatibility-facade", "exception"] {
        assert_eq!(
            lifecycle_violation(state, 13, 13, true, Some(13)),
            Some("train completion requires every slice to be complete")
        );
    }
    assert_eq!(lifecycle_violation("complete", 13, 13, true, None), None);
}

fn collect_rust_files(root: &str) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    collect_recursive(
        Path::new(root),
        &BTreeSet::from(["rs".to_owned()]),
        &mut files,
    );
    files
}

#[test]
fn architecture_roots_are_compile_connected_and_private() {
    let main = fs::read_to_string("src/main.rs").expect("src/main.rs must be readable");
    for root in ARCHITECTURE_ROOTS {
        assert!(main.lines().any(|line| line == format!("mod {root};")));
        assert!(!main.lines().any(|line| line == format!("pub mod {root};")));
    }

    let english = fs::read_to_string("docs/code-architecture.md").unwrap();
    let korean = fs::read_to_string("docs/ko/code-architecture.md").unwrap();
    assert!(english.contains("[코드 아키텍처](ko/code-architecture.md)"));
    assert!(english.contains("[architecture-migration-map.json](architecture-migration-map.json)"));
    assert!(korean.contains("[Code architecture](../code-architecture.md)"));
    assert!(
        korean.contains("[architecture-migration-map.json](../architecture-migration-map.json)")
    );
}

#[test]
fn v0372_foundation_owners_replace_legacy_modules() {
    for target in [
        "src/foundation/error.rs",
        "src/foundation/integrity.rs",
        "src/foundation/serialization.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing foundation owner: {target}"
        );
    }
    for legacy in ["src/checksum.rs", "src/strict_json.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy foundation owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["checksum", "strict_json"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy module remains compile-connected: {legacy_module}"
        );
    }

    let foundation = fs::read_to_string("src/foundation/mod.rs").unwrap();
    for owner in ["error", "integrity", "serialization"] {
        assert!(
            foundation
                .lines()
                .any(|line| line == format!("pub(crate) mod {owner};")),
            "foundation owner is not crate-private: {owner}"
        );
    }

    let app = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        !app.contains("pub struct AppError"),
        "AppError is still owned by command dispatch"
    );
}

#[test]
fn v0372_filesystem_owners_replace_legacy_modules() {
    for target in [
        "src/adapters/filesystem/cache.rs",
        "src/adapters/filesystem/config.rs",
        "src/adapters/filesystem/layout.rs",
        "src/adapters/filesystem/lease.rs",
        "src/adapters/filesystem/windows_replace.rs",
        "src/composition/config.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing filesystem owner: {target}"
        );
    }
    for legacy in [
        "src/cache.rs",
        "src/config.rs",
        "src/lease.rs",
        "src/paths.rs",
        "src/windows_file.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy filesystem owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["cache", "config", "lease", "paths", "windows_file"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy module remains compile-connected: {legacy_module}"
        );
    }

    let filesystem = fs::read_to_string("src/adapters/filesystem/mod.rs").unwrap();
    for owner in ["cache", "config", "layout", "lease", "windows_replace"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            filesystem.lines().any(|line| line == expected),
            "filesystem owner is not crate-private: {owner}"
        );
    }
}

#[test]
fn v0372_terminal_and_platform_owners_replace_legacy_modules() {
    for target in [
        "src/adapters/terminal/capability.rs",
        "src/adapters/terminal/native.rs",
        "tests/surfaces.rs",
        "tests/surfaces/interactive_tui.rs",
        "tests/surfaces/native_terminal.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing terminal owner: {target}"
        );
    }
    for legacy in [
        "src/terminal.rs",
        "tests/interactive_tui.rs",
        "tests/native_terminal.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy terminal owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod terminal;"),
        "legacy terminal module remains compile-connected"
    );

    let terminal = fs::read_to_string("src/adapters/terminal/mod.rs").unwrap();
    for owner in ["capability", "native"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            terminal.lines().any(|line| line == expected),
            "terminal owner is not crate-private: {owner}"
        );
    }
}

#[test]
fn v0373_inference_owners_replace_legacy_domain_and_adapter_slices() {
    for target in [
        "src/runtime_core/inference/backend.rs",
        "src/runtime_core/inference/backend/admission.rs",
        "src/runtime_core/inference/backend/lifecycle.rs",
        "src/runtime_core/inference/benchmark.rs",
        "src/runtime_core/inference/benchmark/fixture.rs",
        "src/runtime_core/inference/benchmark/report.rs",
        "src/runtime_core/inference/model.rs",
        "src/runtime_core/inference/model/codec.rs",
        "src/runtime_core/inference/model/manifest.rs",
        "src/runtime_core/inference/model/promotion.rs",
        "src/runtime_core/inference/resource.rs",
        "src/runtime_core/inference/stream.rs",
        "src/adapters/filesystem/backend_state.rs",
        "src/adapters/filesystem/benchmark_artifact.rs",
        "src/adapters/filesystem/model_artifact.rs",
        "src/adapters/llama_cpp/backend.rs",
        "src/adapters/llama_cpp/install.rs",
        "src/adapters/llama_cpp/stream.rs",
        "src/adapters/process/backend.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing inference owner: {target}"
        );
    }
    for legacy in ["src/backend_stream.rs", "src/resource.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy inference owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["backend_stream", "resource"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy inference module remains compile-connected: {legacy_module}"
        );
    }

    for (facade, moved_definition) in [
        (
            "src/app/inference_adapter/backend.rs",
            "struct BackendSidecarRecord",
        ),
        (
            "src/app/inference_adapter/benchmark.rs",
            "struct BenchmarkFixture",
        ),
        ("src/app/inference_adapter/model.rs", "const CANDIDATES"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved definition: {facade} -> {moved_definition}"
        );
    }

    let backend_adapter_path = "src/app/inference_adapter/backend.rs";
    let backend_chat_path = "src/app/inference_adapter/backend/chat.rs";
    let backend_generation_state_path = "src/app/inference_adapter/backend/generation_state.rs";
    let backend_installation_path = "src/app/inference_adapter/backend/installation.rs";
    let backend_resource_sampling_path = "src/app/inference_adapter/backend/resource_sampling.rs";
    let backend_sidecar_path = "src/app/inference_adapter/backend/sidecar.rs";
    let backend_tests_path = "src/app/inference_adapter/backend/tests.rs";
    assert!(Path::new(backend_chat_path).is_file());
    assert!(Path::new(backend_generation_state_path).is_file());
    assert!(Path::new(backend_installation_path).is_file());
    assert!(Path::new(backend_resource_sampling_path).is_file());
    assert!(Path::new(backend_sidecar_path).is_file());
    assert!(Path::new(backend_tests_path).is_file());
    let backend_adapter = fs::read_to_string(backend_adapter_path).unwrap();
    let backend_chat = fs::read_to_string(backend_chat_path).unwrap();
    let backend_generation_state = fs::read_to_string(backend_generation_state_path).unwrap();
    let backend_installation = fs::read_to_string(backend_installation_path).unwrap();
    let backend_resource_sampling = fs::read_to_string(backend_resource_sampling_path).unwrap();
    let backend_sidecar = fs::read_to_string(backend_sidecar_path).unwrap();
    let backend_tests = fs::read_to_string(backend_tests_path).unwrap();
    assert!(
        backend_adapter.contains("#[path = \"backend/tests.rs\"]"),
        "inference backend adapter does not register its regression-test owner"
    );
    assert!(
        backend_adapter.lines().any(|line| line == "mod chat;"),
        "inference backend adapter does not register its chat owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod generation_state;"),
        "inference backend adapter does not register its generation-state owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod installation;"),
        "inference backend adapter does not register its installation owner"
    );
    assert!(
        backend_adapter
            .lines()
            .any(|line| line == "mod resource_sampling;"),
        "inference backend adapter does not register its resource-sampling owner"
    );
    assert!(
        backend_adapter.lines().any(|line| line == "mod sidecar;"),
        "inference backend adapter does not register its sidecar owner"
    );
    for responsibility in [
        "pub fn chat_report(",
        "pub fn chat_stream_report(",
        "pub fn chat_once(",
        "pub fn chat_once_bounded(",
        "pub fn chat_once_bounded_with_cancel(",
        "pub fn preflight_chat_ready(",
        "pub fn cancel_generation_report(",
        "fn ready_sidecar_record(",
        "fn chat_once_with_options(",
        "fn finish_interrupted_generation(",
    ] {
        assert!(
            backend_chat.contains(responsibility),
            "inference backend chat owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns chat execution: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct ActiveGenerationGuard",
        "pub(super) fn begin_active_generation(",
        "pub(super) fn write_backend_generation_record(",
        "pub(super) fn generation_cancel_requested(",
        "pub(super) fn write_generation_cancel_marker(",
        "pub(super) fn write_generation_terminal_record(",
        "pub(super) fn wait_for_generation_terminal(",
        "pub(super) fn release_generation_admission(",
    ] {
        assert!(
            backend_generation_state.contains(responsibility),
            "inference backend generation-state owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns generation state: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn install_plan_report(",
        "pub fn install_report(",
        "pub fn verify_archive_report(",
        "pub(super) fn install_backend_from_archive(",
    ] {
        assert!(
            backend_installation.contains(responsibility),
            "inference backend installation owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct BackendResourceSampleReport",
        "pub(super) fn display_optional_f64(",
        "pub(super) fn display_optional_u64_unknown(",
        "fn backend_resource_paths(",
        "pub(super) fn record_backend_resource_sample(",
    ] {
        assert!(
            backend_resource_sampling.contains(responsibility),
            "inference backend resource-sampling owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns resource sampling: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn doctor_report(",
        "pub fn start_report(",
        "pub fn status_report(",
        "pub fn stop_report(",
        "pub fn health_check_report(",
        "pub(super) fn terminate_with_fallback(",
        "pub(super) fn cancel_active_generation_before_stop(",
        "pub(super) fn start_sidecar_with_timeout(",
        "pub(super) fn trace_backend_start(",
    ] {
        assert!(
            backend_sidecar.contains(responsibility),
            "inference backend sidecar owner is missing: {responsibility}"
        );
        assert!(
            !backend_adapter.contains(responsibility),
            "inference backend facade still owns sidecar lifecycle: {responsibility}"
        );
    }
    for responsibility in [
        "fn release_manifest_has_source_backed_supported_artifacts(",
        "fn generation_record_codec_preserves_exact_bytes_and_round_trips(",
        "fn parallel_generation_cancel_reaches_secondary_and_keeps_state_until_last_release(",
        "fn start_timeout_removes_record_and_keeps_logs(",
    ] {
        assert!(
            backend_tests.contains(responsibility),
            "inference backend regression owner is missing: {responsibility}"
        );
    }
    assert!(
        backend_adapter.lines().count() < 125,
        "inference backend production adapter regrew beyond its resource-sampling extraction boundary"
    );
    assert!(
        backend_chat.lines().count() < 750,
        "inference backend chat module regrew beyond its ownership boundary"
    );
    assert!(
        backend_generation_state.lines().count() < 250,
        "inference backend generation-state module regrew beyond its ownership boundary"
    );
    assert!(
        backend_installation.lines().count() < 225,
        "inference backend installation module regrew beyond its ownership boundary"
    );
    assert!(
        backend_resource_sampling.lines().count() < 110,
        "inference backend resource-sampling module regrew beyond its ownership boundary"
    );
    assert!(
        backend_sidecar.lines().count() < 650,
        "inference backend sidecar module regrew beyond its ownership boundary"
    );
    assert!(
        backend_tests.lines().count() < 900,
        "inference backend regression module regrew beyond its ownership boundary"
    );
}

#[test]
fn v0375_domain_views_replace_legacy_definitions() {
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let transcript_adapter = "src/app/workflow_adapter/transcript.rs";
    for target in [
        "src/runtime_core/workflow/domain/mod.rs",
        "src/runtime_core/workflow/domain/snapshot.rs",
        "src/runtime_core/workflow/domain/transcript.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing domain owner: {target}"
        );
    }

    let domain = fs::read_to_string("src/runtime_core/workflow/domain/mod.rs").unwrap();
    for owner in ["snapshot", "transcript"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            domain.lines().any(|line| line == expected),
            "workflow domain owner is not crate-private: {owner}"
        );
    }

    for (facade, moved_definition) in [
        (state_adapter, "struct CurrentStateSnapshot"),
        (state_adapter, "struct CurrentStateLeaseView"),
        (transcript_adapter, "struct ToolOutputView"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved definition: {facade} -> {moved_definition}"
        );
    }

    let snapshot = fs::read_to_string("src/runtime_core/workflow/domain/snapshot.rs").unwrap();
    for rule in [
        "fn validate_session_resume_target",
        "fn validate_current_lease",
        "fn validate_read_only_workflow",
    ] {
        assert!(
            snapshot.contains(rule),
            "snapshot owner is missing domain rule: {rule}"
        );
    }

    assert!(
        !Path::new("src/state.rs").exists(),
        "legacy workflow root was restored: src/state.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod state;"),
        "legacy workflow root remains registered: mod state;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod state;"),
        "state adapter is not registered under workflow_adapter"
    );

    let transcript = fs::read_to_string("src/runtime_core/workflow/domain/transcript.rs").unwrap();
    for rule in [
        "fn collect_session_records",
        "fn parse_event_binding",
        "fn validate_event_identity",
    ] {
        assert!(
            transcript.contains(rule),
            "transcript owner is missing domain rule: {rule}"
        );
    }

    assert!(
        !Path::new("src/transcript.rs").exists(),
        "legacy workflow root was restored: src/transcript.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod transcript;"),
        "legacy workflow root remains registered: mod transcript;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod transcript;"),
        "transcript adapter is not registered under workflow_adapter"
    );
}

#[test]
fn v0376_workflow_application_owns_transaction_and_recovery_order() {
    let ledger_adapter = "src/app/workflow_adapter/ledger.rs";
    let transition_adapter = "src/app/workflow_adapter/transition.rs";
    for target in [
        transition_adapter,
        "src/runtime_core/workflow/application/mod.rs",
        "src/runtime_core/workflow/application/recovery.rs",
        "src/runtime_core/workflow/application/transaction_coordinator.rs",
        "src/runtime_core/workflow/domain/transition.rs",
        "tests/workflow/recovery.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing workflow transaction/recovery owner: {target}"
        );
    }

    let workflow = fs::read_to_string("src/runtime_core/workflow/mod.rs").unwrap();
    assert!(
        workflow
            .lines()
            .any(|line| line == "pub(crate) mod application;"),
        "workflow application owner is not crate-private"
    );
    let application = fs::read_to_string("src/runtime_core/workflow/application/mod.rs").unwrap();
    for owner in ["recovery", "transaction_coordinator"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            application.lines().any(|line| line == expected),
            "workflow application owner is not crate-private: {owner}"
        );
    }

    let coordinator =
        fs::read_to_string("src/runtime_core/workflow/application/transaction_coordinator.rs")
            .unwrap();
    for rule in [
        "fn execute_approval_transaction",
        "fn execute_verification_transaction",
        "fn execute_terminal_action_transaction",
        "fn execute_state_transition",
        "fn execute_reconcile_transaction",
    ] {
        assert!(
            coordinator.contains(rule),
            "transaction coordinator is missing ordered use case: {rule}"
        );
    }

    let recovery = fs::read_to_string("src/runtime_core/workflow/application/recovery.rs").unwrap();
    for rule in [
        "fn recover_workflow_transaction",
        "fn recover_prepared_state_transition",
    ] {
        assert!(
            recovery.contains(rule),
            "workflow recovery owner is missing policy: {rule}"
        );
    }

    for (facade, moved_definition) in [
        (ledger_adapter, "struct PlannedEvent"),
        (transition_adapter, "enum CurrentStateIntent"),
        (transition_adapter, "struct PreparedSourceBundle"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved workflow definition: {facade} -> {moved_definition}"
        );
    }

    assert!(
        !Path::new("src/ledger.rs").exists(),
        "legacy workflow root was restored: src/ledger.rs"
    );
    assert!(
        !Path::new("src/transition.rs").exists(),
        "legacy workflow root was restored: src/transition.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod ledger;"),
        "legacy workflow root remains registered: mod ledger;"
    );
    assert!(
        !main.lines().any(|line| line == "mod transition;"),
        "legacy workflow root remains registered: mod transition;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod ledger;"),
        "ledger adapter is not registered under workflow_adapter"
    );
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod transition;"),
        "transition adapter is not registered under workflow_adapter"
    );

    let patch_loop = fs::read_to_string("tests/patch_loop.rs").unwrap();
    let patch_lifecycle = fs::read_to_string("tests/patch/lifecycle.rs").unwrap();
    assert!(
        patch_loop.contains("#[path = \"patch/lifecycle.rs\"]")
            && patch_lifecycle.contains("#[path = \"../workflow/recovery.rs\"]"),
        "patch-loop recovery filters are not owned by tests/workflow/recovery.rs"
    );
}

#[test]
fn v03713_transition_adapter_delegates_source_install_contract() {
    let transition_adapter = "src/app/workflow_adapter/transition.rs";
    let bundle_codec_adapter = "src/app/workflow_adapter/transition/bundle_codec.rs";
    let bundle_preparation_adapter = "src/app/workflow_adapter/transition/bundle_preparation.rs";
    let bundle_validation_adapter = "src/app/workflow_adapter/transition/bundle_validation.rs";
    let journal_adapter = "src/app/workflow_adapter/transition/journal.rs";
    let source_install_adapter = "src/app/workflow_adapter/transition/source_install.rs";
    let transition_tests = "src/app/workflow_adapter/transition/tests/mod.rs";
    for target in [
        transition_adapter,
        bundle_codec_adapter,
        bundle_preparation_adapter,
        bundle_validation_adapter,
        journal_adapter,
        source_install_adapter,
        transition_tests,
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing transition adapter owner: {target}"
        );
    }

    let transition = fs::read_to_string(transition_adapter).unwrap();
    let bundle_codec = fs::read_to_string(bundle_codec_adapter).unwrap();
    let bundle_preparation = fs::read_to_string(bundle_preparation_adapter).unwrap();
    let bundle_validation = fs::read_to_string(bundle_validation_adapter).unwrap();
    let journal = fs::read_to_string(journal_adapter).unwrap();
    let source_install = fs::read_to_string(source_install_adapter).unwrap();
    let tests = fs::read_to_string(transition_tests).unwrap();
    assert!(
        transition.lines().any(|line| line == "mod bundle_codec;"),
        "transition adapter does not register the bundle-codec owner"
    );
    for responsibility in [
        "pub(super) fn render_source_members(",
        "pub(super) fn parse_source_members(",
        "pub(super) struct PreparedMemberParseContext",
        "pub(super) fn parse_semantic_events(",
        "pub(super) fn parse_event_chain_plan(",
        "pub(super) fn prepared_member_order(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-codec responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_codec.contains(responsibility),
            "bundle-codec adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition
            .lines()
            .any(|line| line == "mod bundle_preparation;"),
        "transition adapter does not register the bundle-preparation owner"
    );
    for responsibility in [
        "pub(crate) fn prepare_state_transition_bundle(",
        "pub(crate) fn prepare_source_bundle_with_context(",
        "pub(crate) fn prepare_projection_lag_member(",
        "pub(crate) fn install_projection_lag(",
        "pub(crate) fn bind_planned_events(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-preparation responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_preparation.contains(responsibility),
            "bundle-preparation adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition
            .lines()
            .any(|line| line == "mod bundle_validation;"),
        "transition adapter does not register the bundle-validation owner"
    );
    for responsibility in [
        "pub(super) fn validate_prepared_source_bundle(",
        "pub(super) fn validate_event_chain(",
        "fn validate_additional_members(",
        "fn validate_state_transition_members(",
        "fn validate_verification_members(",
        "fn validate_projection_lag_member(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-validation responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_validation.contains(responsibility),
            "bundle-validation adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.lines().any(|line| line == "mod journal;"),
        "transition adapter does not register the journal owner"
    );
    for responsibility in [
        "pub(crate) struct TransitionGuard",
        "pub(crate) fn render_prepared_source_bundle(",
        "pub(crate) fn parse_prepared_source_bundle(",
        "pub(crate) fn commit_prepared_source_bundle(",
        "pub(crate) fn recover_pending_source_bundles(",
        "fn recover_pending_bundles_under_guard(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "journal responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            journal.contains(responsibility),
            "transition journal adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.lines().any(|line| line == "mod source_install;"),
        "transition adapter does not register the source-install owner"
    );
    assert!(
        transition
            .lines()
            .any(|line| line == "pub(crate) use source_install::{"),
        "transition adapter does not expose the source-install contract"
    );
    for responsibility in [
        "pub(crate) fn prepare_source_install_v1(",
        "pub(crate) fn validate_source_install_v1(",
        "pub(crate) fn render_source_install_v1(",
        "pub(crate) fn parse_source_install_v1(",
        "pub(crate) fn source_identity_v1(",
        "pub(crate) fn resolve_prepared_project_path(",
        "pub(crate) fn source_install_rollback_path(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "source-install responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            source_install.contains(responsibility),
            "source-install adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.contains("#[path = \"transition/tests/mod.rs\"]"),
        "transition adapter does not register its regression test owner"
    );
    for responsibility in [
        "fn recovery_enforces_file_and_directory_read_bounds_before_parsing(",
        "fn source_install_v1_round_trips_exact_order_and_bindings(",
        "fn prepared_bundle_strictly_binds_semantic_event_chain_plan(",
    ] {
        assert!(
            tests.contains(responsibility),
            "transition regression tests are missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.lines().count() < 625,
        "transition adapter regrew beyond its extracted ownership boundary"
    );
    assert!(
        bundle_codec.lines().count() < 550,
        "bundle-codec adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_preparation.lines().count() < 500,
        "bundle-preparation adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_validation.lines().count() < 725,
        "bundle-validation adapter regrew beyond its ownership boundary"
    );
    assert!(
        journal.lines().count() < 900,
        "transition journal adapter regrew beyond its ownership boundary"
    );
    assert!(
        source_install.lines().count() < 500,
        "source-install adapter regrew beyond its ownership boundary"
    );
    assert!(
        tests.lines().count() < 750,
        "transition regression tests regrew beyond their ownership boundary"
    );
}

#[test]
fn v0377_observability_ports_own_projection_and_monitoring_boundaries() {
    for target in [
        "src/adapters/sqlite/ledger_projection.rs",
        "src/adapters/sqlite/observability_projection.rs",
        "src/adapters/sqlite/transcript_projection.rs",
        "src/runtime_core/observability/facade.rs",
        "src/runtime_core/observability/monitor.rs",
        "src/runtime_core/workflow/application/projection_barrier.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.7 observability owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod observability;"),
        "runtime observability owner is not crate-private"
    );
    let observability_mod = fs::read_to_string("src/runtime_core/observability/mod.rs").unwrap();
    for owner in ["facade", "monitor"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            observability_mod.lines().any(|line| line == expected),
            "runtime observability child is not crate-private: {owner}"
        );
    }

    let facade = fs::read_to_string("src/runtime_core/observability/facade.rs").unwrap();
    assert!(
        facade.contains("trait ObservabilityProjectionPort"),
        "observability facade does not own the projection port"
    );
    assert!(
        facade.contains("trait CanonicalLedgerReadPort"),
        "observability facade does not own the canonical ledger read port"
    );
    assert!(
        facade.contains("trait CanonicalTranscriptReadPort")
            && facade.contains("trait CanonicalProjectionReadPort"),
        "observability facade does not own the canonical transcript projection port"
    );
    for record in [
        "struct StoreStatus",
        "struct MonitorProjectionSnapshot",
        "struct ModelRunMetric",
        "struct SessionHistoryEntry",
    ] {
        assert!(
            facade.contains(record),
            "observability facade is missing projection record: {record}"
        );
    }

    let monitor = fs::read_to_string("src/runtime_core/observability/monitor.rs").unwrap();
    for rule in [
        "trait MonitorQueryPort",
        "fn status_report",
        "fn models_report",
        "fn baseline_report",
        "fn optimize_report",
        "fn prune_report",
    ] {
        assert!(
            monitor.contains(rule),
            "monitor owner is missing use case: {rule}"
        );
    }

    let sqlite = fs::read_to_string("src/adapters/sqlite/observability_projection.rs").unwrap();
    let analytics_path = "src/adapters/sqlite/observability_projection/analytics.rs";
    let metrics_path = "src/adapters/sqlite/observability_projection/metrics.rs";
    let read_snapshot_path = "src/adapters/sqlite/observability_projection/read_snapshot.rs";
    let replay_path = "src/adapters/sqlite/observability_projection/replay.rs";
    let schema_path = "src/adapters/sqlite/observability_projection/schema.rs";
    let sqlite_tests_path = "src/adapters/sqlite/observability_projection/tests.rs";
    assert!(Path::new(analytics_path).is_file());
    assert!(Path::new(metrics_path).is_file());
    assert!(Path::new(read_snapshot_path).is_file());
    assert!(Path::new(replay_path).is_file());
    assert!(Path::new(schema_path).is_file());
    assert!(Path::new(sqlite_tests_path).is_file());
    let analytics = fs::read_to_string(analytics_path).unwrap();
    let metrics = fs::read_to_string(metrics_path).unwrap();
    let read_snapshot = fs::read_to_string(read_snapshot_path).unwrap();
    let replay = fs::read_to_string(replay_path).unwrap();
    let schema = fs::read_to_string(schema_path).unwrap();
    let sqlite_tests = fs::read_to_string(sqlite_tests_path).unwrap();
    for rule in ["impl ObservabilityProjectionPort for SqliteObservabilityProjection"] {
        assert!(sqlite.contains(rule), "SQLite adapter is missing: {rule}");
    }
    assert!(
        replay.contains("pub(super) fn replay_ledger_events("),
        "SQLite replay owner is missing canonical replay"
    );
    assert!(
        schema.contains("PRAGMA journal_mode = WAL"),
        "SQLite schema owner is missing WAL migration policy"
    );
    let sqlite_production = sqlite.split("#[cfg(test)]").next().unwrap_or(&sqlite);
    assert!(
        !sqlite_production.contains("crate::ledger"),
        "SQLite projection adapter bypasses the consumer-owned projection port"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod analytics;"),
        "SQLite projection does not register the analytics owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod metrics;"),
        "SQLite projection does not register the metric owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod read_snapshot;"),
        "SQLite projection does not register the read-only snapshot owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod replay;"),
        "SQLite projection does not register the replay owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod schema;"),
        "SQLite projection does not register the schema owner"
    );
    for responsibility in ["pub(super) fn migrate(", "fn ensure_column("] {
        assert!(
            !sqlite.contains(responsibility),
            "schema responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            schema.contains(responsibility),
            "SQLite schema owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn model_summaries_from_connection(",
        "pub(super) fn model_summaries(",
        "pub(super) fn performance_baseline(",
        "pub(super) fn optimization_policy(",
        "fn query_baseline_model_rows(",
        "fn benchmark_evidence_summary(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "analytics responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            analytics.contains(responsibility),
            "SQLite analytics owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn record_model_run(",
        "pub(super) fn record_resource_sample(",
        "pub(super) fn record_benchmark_run(",
        "pub(super) fn benchmark_run_reports(",
        "pub(super) fn latest_resource_sample(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "metric responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            metrics.contains(responsibility),
            "SQLite metric owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn record_session(",
        "pub(super) fn replay_ledger_events(",
        "pub(super) fn project_sessions_from_events(",
        "pub(super) fn insert_ledger_event(",
        "pub(super) fn project_workflow_checkpoint(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "replay responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            replay.contains(responsibility),
            "SQLite replay owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct ReadOnlyProjection",
        "pub(super) fn open_read_only(",
        "pub(super) fn open_read_only_path(",
        "fn stable_projection_files(",
        "fn read_regular_snapshot_file(",
        "fn write_private_snapshot_file(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "read-only snapshot responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            read_snapshot.contains(responsibility),
            "read-only snapshot owner is missing: {responsibility}"
        );
    }
    assert!(
        sqlite.contains("#[path = \"observability_projection/tests.rs\"]"),
        "SQLite projection does not register its regression-test owner"
    );
    for responsibility in [
        "fn corrupt_sqlite_is_preserved_before_canonical_ledger_failure(",
        "fn sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows(",
        "fn performance_baseline_aggregates_local_metrics(",
        "fn optimization_policy_reads_metrics_and_measured_benchmark_evidence(",
    ] {
        assert!(
            sqlite_tests.contains(responsibility),
            "SQLite projection regression owner is missing: {responsibility}"
        );
    }
    assert!(
        sqlite.lines().count() < 650,
        "SQLite projection production module regrew beyond its analytics extraction boundary"
    );
    assert!(
        analytics.lines().count() < 450,
        "SQLite analytics module regrew beyond its ownership boundary"
    );
    assert!(
        metrics.lines().count() < 375,
        "SQLite metric module regrew beyond its ownership boundary"
    );
    assert!(
        read_snapshot.lines().count() < 275,
        "SQLite read-only snapshot module regrew beyond its ownership boundary"
    );
    assert!(
        replay.lines().count() < 375,
        "SQLite replay module regrew beyond its ownership boundary"
    );
    assert!(
        schema.lines().count() < 400,
        "SQLite schema module regrew beyond its ownership boundary"
    );
    assert!(
        sqlite_tests.lines().count() < 825,
        "SQLite projection regression module regrew beyond its ownership boundary"
    );

    let transcript = fs::read_to_string("src/adapters/sqlite/transcript_projection.rs").unwrap();
    assert!(
        transcript.contains("INSERT OR REPLACE INTO transcript_records"),
        "transcript SQLite adapter does not own row installation"
    );
    assert!(
        transcript.contains("CanonicalTranscriptReadPort") && !transcript.contains("crate::app"),
        "transcript SQLite adapter does not use the inverted canonical read port"
    );
    let ledger = fs::read_to_string("src/adapters/sqlite/ledger_projection.rs").unwrap();
    assert!(
        ledger.contains("fn validate_event_sequence"),
        "ledger SQLite adapter does not own sequence validation"
    );

    let barrier =
        fs::read_to_string("src/runtime_core/workflow/application/projection_barrier.rs").unwrap();
    for rule in [
        "trait ProjectionBarrierRecoveryPort",
        "fn recover_through_projection_barrier",
    ] {
        assert!(
            barrier.contains(rule),
            "projection barrier owner is missing policy: {rule}"
        );
    }
    let recovery = fs::read_to_string("src/runtime_core/workflow/application/recovery.rs").unwrap();
    assert!(
        !recovery.contains("fn recover_through_projection_barrier"),
        "workflow recovery still owns the moved projection barrier"
    );

    for (facade_path, forbidden) in [
        ("src/app/observability_adapter.rs", "rusqlite"),
        ("src/app/monitor_adapter.rs", "performance baseline\\n"),
        ("src/app/workflow_adapter/ledger.rs", "rusqlite::Connection"),
    ] {
        let source = fs::read_to_string(facade_path).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved implementation: {facade_path} -> {forbidden}"
        );
    }
    assert!(!Path::new("src/monitor.rs").exists());
    let monitor_adapter = fs::read_to_string("src/app/monitor_adapter.rs").unwrap();
    assert!(monitor_adapter.contains("impl MonitorQueryPort for LocalMonitorQueryPort"));
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod monitor;"));
    assert!(!Path::new("src/observability.rs").exists());
    let observability_adapter = fs::read_to_string("src/app/observability_adapter.rs").unwrap();
    assert!(observability_adapter.contains("impl CanonicalLedgerReadPort"));
    assert!(observability_adapter.contains("impl CanonicalTranscriptReadPort"));
    assert!(!main.lines().any(|line| line == "mod observability;"));
}

#[test]
fn v0378_knowledge_and_policy_owners_hold_domain_rules() {
    let owners = [
        "src/runtime_core/knowledge/context.rs",
        "src/runtime_core/knowledge/evidence.rs",
        "src/runtime_core/knowledge/ontology.rs",
        "src/runtime_core/policy/approval.rs",
        "src/runtime_core/policy/decision.rs",
    ];
    for target in owners {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.8 knowledge/policy owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    for owner in ["knowledge", "policy"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            runtime_core.lines().any(|line| line == expected),
            "runtime owner is not crate-private: {owner}"
        );
    }
    for (module, children) in [
        (
            "src/runtime_core/knowledge/mod.rs",
            ["context", "evidence", "ontology"].as_slice(),
        ),
        (
            "src/runtime_core/policy/mod.rs",
            ["approval", "decision"].as_slice(),
        ),
    ] {
        let source = fs::read_to_string(module).unwrap();
        for child in children {
            let expected = format!("pub(crate) mod {child};");
            assert!(
                source.lines().any(|line| line == expected),
                "runtime child is not crate-private: {module} -> {child}"
            );
        }
    }

    for (owner, rules) in [
        (
            "src/runtime_core/knowledge/context.rs",
            [
                "struct ContextPack",
                "struct ResumeContext",
                "fn enforce_shared_source_budget",
                "fn truncate_tail_chars",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/knowledge/evidence.rs",
            [
                "struct StopGateInputs",
                "fn validate_stop_inputs",
                "fn validate_artifact_pointer_syntax",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/knowledge/ontology.rs",
            [
                "struct OntologyRecord",
                "fn parse_projection",
                "fn runtime_context_selection",
                "fn validate_import_text",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/policy/approval.rs",
            [
                "struct ApprovalRequest",
                "fn render_request_record",
                "fn validate_request_id",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/policy/decision.rs",
            [
                "enum Decision",
                "trait PathPolicyPort",
                "fn classify_command",
                "fn classify_path",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.8 owner is missing domain rule: {owner} -> {rule}"
            );
        }
        for forbidden in ["crate::adapters", "crate::ledger", "crate::state"] {
            assert!(
                !source.contains(forbidden),
                "runtime knowledge/policy owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    let policy_facade = fs::read_to_string("src/app/policy_adapter.rs").unwrap();
    assert!(
        policy_facade.contains("impl PathPolicyPort for ProjectPathPolicy"),
        "filesystem path policy is not composed through the consumer-owned port"
    );

    for (facade, forbidden) in [
        ("src/app/approval_adapter.rs", "struct ApprovalRequest"),
        ("src/app/approval_adapter.rs", "fn render_request_record"),
        ("src/context.rs", "pub struct ContextPack"),
        ("src/context.rs", "fn clamp_source_pack"),
        ("src/evidence.rs", "struct StopGateInputs"),
        ("src/evidence.rs", "fn stale_policy_summary"),
        ("src/ontology.rs", "struct OntologyRecord"),
        ("src/ontology.rs", "fn select_context_records"),
        ("src/app/policy_adapter.rs", "pub enum Decision"),
        (
            "src/app/policy_adapter.rs",
            "fn validate_patch_verification_argv",
        ),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved knowledge/policy rule: {facade} -> {forbidden}"
        );
    }
    assert!(!Path::new("src/approval.rs").exists());
    let approval_adapter = fs::read_to_string("src/app/approval_adapter.rs").unwrap();
    assert!(approval_adapter.contains("pub fn write_request"));
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod approval;"));
    assert!(!Path::new("src/policy.rs").exists());
    assert!(policy_facade.contains("impl PathPolicyPort for ProjectPathPolicy"));
    assert!(!main.lines().any(|line| line == "mod policy;"));
}

#[test]
fn v0379_patch_owners_hold_lifecycle_decisions() {
    let patch_test_modules = [
        "src/patch/tests/mod.rs",
        "src/patch/tests/approval_cases.rs",
        "src/patch/tests/recovery_cases.rs",
        "src/patch/tests/support_cases.rs",
        "src/patch/tests/terminal_cases.rs",
        "src/patch/tests/verification_cases.rs",
    ];
    let approval_transaction_adapter = "src/patch/approval_transaction.rs";
    let execution_adapter = "src/patch/execution.rs";
    let guard_adapter = "src/patch/guard.rs";
    let proposal_builder_adapter = "src/patch/proposal_builder.rs";
    let proposal_store_adapter = "src/patch/proposal_store.rs";
    let resume_adapter = "src/patch/resume.rs";
    let terminal_adapter = "src/patch/terminal.rs";
    let verification_adapter = "src/patch/verification.rs";
    let workflow_contract_adapter = "src/patch/workflow_contract.rs";
    let workflow_execution_adapter = "src/patch/workflow_execution.rs";
    assert!(Path::new(approval_transaction_adapter).is_file());
    assert!(Path::new(execution_adapter).is_file());
    assert!(Path::new(guard_adapter).is_file());
    assert!(Path::new(proposal_builder_adapter).is_file());
    assert!(Path::new(proposal_store_adapter).is_file());
    assert!(Path::new(resume_adapter).is_file());
    assert!(Path::new(terminal_adapter).is_file());
    assert!(Path::new(verification_adapter).is_file());
    assert!(Path::new(workflow_contract_adapter).is_file());
    assert!(Path::new(workflow_execution_adapter).is_file());
    for patch_test_module in patch_test_modules {
        assert!(
            Path::new(patch_test_module).is_file(),
            "missing patch regression test owner: {patch_test_module}"
        );
    }
    let owners = [
        "src/runtime_core/patch/approval.rs",
        "src/runtime_core/patch/application.rs",
        "src/runtime_core/patch/intent.rs",
        "src/runtime_core/patch/proposal.rs",
        "src/runtime_core/patch/verification.rs",
    ];
    for target in owners {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.9 patch owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod patch;"),
        "patch runtime owner is not crate-private"
    );
    let patch_mod = fs::read_to_string("src/runtime_core/patch/mod.rs").unwrap();
    for child in [
        "approval",
        "application",
        "intent",
        "proposal",
        "verification",
    ] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            patch_mod.lines().any(|line| line == expected),
            "patch child is not crate-private: {child}"
        );
    }

    for (owner, rules) in [
        (
            "src/runtime_core/patch/approval.rs",
            ["fn token_from_entropy", "fn hash_token", "fn matches_hash"].as_slice(),
        ),
        (
            "src/runtime_core/patch/application.rs",
            [
                "enum ApplyAdmission",
                "fn admit_apply",
                "fn admit_rollback",
                "fn validate_applied_source",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/patch/intent.rs",
            [
                "struct IntentDecision",
                "fn classify",
                "fn plan_action_candidate",
                "fn parse_model_action",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/patch/proposal.rs",
            [
                "struct PatchPreview",
                "fn build_preview",
                "fn render_record",
                "fn parse_record",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/patch/verification.rs",
            [
                "struct VerificationPlan",
                "enum RecoveryAdmission",
                "fn build_plan",
                "fn recovery_admission",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.9 owner is missing lifecycle rule: {owner} -> {rule}"
            );
        }
        for forbidden in [
            "crate::adapters",
            "crate::ledger",
            "crate::state",
            "crate::runtime::",
            "crate::skill",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(forbidden),
                "patch owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    for (facade, forbidden) in [
        ("src/intent.rs", "struct IntentDecision"),
        ("src/intent.rs", "fn plan_action_candidate"),
        ("src/intent.rs", "fn parse_model_action"),
        ("src/patch.rs", "struct PatchPreview"),
        ("src/patch.rs", "struct ProposalRecord"),
        ("src/patch.rs", "struct ApplyResult"),
        ("src/patch.rs", "struct RollbackResult"),
        ("src/patch.rs", "struct VerificationPlan"),
        ("src/patch.rs", "struct VerificationResult"),
        ("src/patch.rs", "fn render_unified_diff"),
        ("src/patch.rs", "fn parse_proposal_header"),
        ("src/patch.rs", "fn constant_time_eq"),
        ("src/patch.rs", "fn is_test_verification"),
        ("src/patch.rs", "fn output_excerpt"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved patch rule: {facade} -> {forbidden}"
        );
    }

    let intent_facade = fs::read_to_string("src/intent.rs").unwrap();
    let patch_facade = fs::read_to_string("src/patch.rs").unwrap();
    let approval_transaction = fs::read_to_string(approval_transaction_adapter).unwrap();
    let execution = fs::read_to_string(execution_adapter).unwrap();
    let guard = fs::read_to_string(guard_adapter).unwrap();
    let proposal_builder = fs::read_to_string(proposal_builder_adapter).unwrap();
    let proposal_store = fs::read_to_string(proposal_store_adapter).unwrap();
    let resume = fs::read_to_string(resume_adapter).unwrap();
    let terminal = fs::read_to_string(terminal_adapter).unwrap();
    let verification = fs::read_to_string(verification_adapter).unwrap();
    let workflow_contract = fs::read_to_string(workflow_contract_adapter).unwrap();
    let workflow_execution = fs::read_to_string(workflow_execution_adapter).unwrap();
    let patch_test_module = fs::read_to_string(patch_test_modules[0]).unwrap();
    let patch_approval_tests = fs::read_to_string(patch_test_modules[1]).unwrap();
    let patch_recovery_tests = fs::read_to_string(patch_test_modules[2]).unwrap();
    let patch_support_tests = fs::read_to_string(patch_test_modules[3]).unwrap();
    let patch_terminal_tests = fs::read_to_string(patch_test_modules[4]).unwrap();
    let patch_verification_tests = fs::read_to_string(patch_test_modules[5]).unwrap();
    let patch_harness = fs::read_to_string("tests/patch_loop.rs").unwrap();
    let patch_contract = fs::read_to_string("tests/patch/lifecycle.rs").unwrap();
    assert!(
        intent_facade.lines().count() <= 1_400,
        "intent facade regrew beyond the v0.37.9 boundary"
    );
    assert!(
        patch_facade.lines().count() < 500,
        "patch facade regrew beyond the v0.37.9 boundary"
    );
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod approval_transaction;"));
    for escaped_responsibility in [
        "fn approve_prepared_skill_transaction(",
        "fn recover_prepared_approval_bundle(",
        "fn validate_prepared_approval_semantics(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "approval transaction responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            approval_transaction.contains(escaped_responsibility),
            "approval transaction adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(approval_transaction.lines().count() < 900);
    assert!(patch_facade.lines().any(|line| line == "mod execution;"));
    for escaped_responsibility in [
        "fn apply_proposal(",
        "fn run_verification(",
        "fn restore_from_rollback(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "patch execution responsibility escaped into facade: {escaped_responsibility}"
        );
        assert!(
            execution.contains(escaped_responsibility),
            "patch execution adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(execution.lines().count() < 300);
    assert!(patch_facade.lines().any(|line| line == "mod guard;"));
    for escaped_responsibility in [
        "struct ApprovalLock",
        "fn approval_transaction_fault(",
        "fn restore_bytes(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "patch guard responsibility escaped into facade: {escaped_responsibility}"
        );
        assert!(
            guard.contains(escaped_responsibility),
            "patch guard adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(guard.lines().count() < 250);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod proposal_builder;"));
    for escaped_responsibility in [
        "fn build_preview(",
        "struct TargetPath",
        "fn fill_os_random(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "proposal builder responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            proposal_builder.contains(escaped_responsibility),
            "proposal builder adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(proposal_builder.lines().count() < 250);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod proposal_store;"));
    for escaped_responsibility in [
        "fn read_proposal_contents_bounded(",
        "fn load_proposal_record(",
        "fn validate_token_hash(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "proposal store responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            proposal_store.contains(escaped_responsibility),
            "proposal store adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(proposal_store.lines().count() < 350);
    assert!(patch_facade.lines().any(|line| line == "mod resume;"));
    for escaped_responsibility in [
        "fn proposal_summaries_bounded(",
        "fn preflight_resume_workflow(",
        "fn resume_workflow_for_tui(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "resume responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            resume.contains(escaped_responsibility),
            "resume adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(resume.lines().count() < 400);
    assert!(patch_facade.lines().any(|line| line == "mod terminal;"));
    for escaped_responsibility in [
        "fn cancel_workflow_transaction(",
        "fn deny_pending_gate_transaction(",
        "fn prepare_terminal_rollback_source(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "terminal workflow responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            terminal.contains(escaped_responsibility),
            "terminal workflow adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(terminal.lines().count() < 500);
    assert!(patch_facade.lines().any(|line| line == "mod verification;"));
    for escaped_responsibility in [
        "fn verify_report_for_intent(",
        "fn approve_prepared_verification_transaction(",
        "fn prepared_verification_members(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "verification responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            verification.contains(escaped_responsibility),
            "verification adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(verification.lines().count() < 300);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod workflow_contract;"));
    for escaped_responsibility in [
        "fn stale_selection_error(",
        "fn validate_workflow_binding(",
        "fn success_report(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "workflow contract responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            workflow_contract.contains(escaped_responsibility),
            "workflow contract adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(workflow_contract.lines().count() < 150);
    assert!(patch_facade
        .lines()
        .any(|line| line == "mod workflow_execution;"));
    for escaped_responsibility in [
        "fn continue_approved_workflow(",
        "fn validate_completed_plugin_workflow(",
        "fn dispatch_workflow_skill_hook(",
    ] {
        assert!(
            !patch_facade.contains(escaped_responsibility),
            "workflow execution responsibility escaped into patch facade: {escaped_responsibility}"
        );
        assert!(
            workflow_execution.contains(escaped_responsibility),
            "workflow execution adapter is missing responsibility: {escaped_responsibility}"
        );
    }
    assert!(workflow_execution.lines().count() < 650);
    assert!(patch_facade.contains("#[path = \"patch/tests/mod.rs\"]"));
    assert!(!patch_facade.contains("mod tests {"));
    assert!(
        patch_test_module.lines().count() < 150,
        "shared patch test fixtures regrew beyond their boundary"
    );
    for module in [
        "mod approval_cases;",
        "mod recovery_cases;",
        "mod support_cases;",
        "mod terminal_cases;",
        "mod verification_cases;",
    ] {
        assert!(
            patch_test_module.lines().any(|line| line == module),
            "shared patch test module is missing child ownership: {module}"
        );
    }
    for (owner, source, marker) in [
        (
            patch_test_modules[1],
            &patch_approval_tests,
            "fn prepared_skill_approval_commits_exact_e0_e9_and_single_current_revision",
        ),
        (
            patch_test_modules[2],
            &patch_recovery_tests,
            "fn prepared_bundle_member_tamper_blocks_recovery_before_effects",
        ),
        (
            patch_test_modules[3],
            &patch_support_tests,
            "fn rollback_tamper_and_replace_failure_are_reported_truthfully",
        ),
        (
            patch_test_modules[4],
            &patch_terminal_tests,
            "fn terminal_denial_crash_matrix_recovers_one_exact_commit",
        ),
        (
            patch_test_modules[5],
            &patch_verification_tests,
            "fn verification_runs_only_after_separate_approval",
        ),
    ] {
        assert!(
            source.lines().count() < 700,
            "patch regression test owner regrew beyond its boundary: {owner}"
        );
        assert!(
            source.contains(marker),
            "patch regression test owner is missing responsibility: {owner} -> {marker}"
        );
    }
    assert!(
        patch_harness.lines().count() <= 5 && patch_harness.contains("patch/lifecycle.rs"),
        "patch integration harness is not a thin compatibility entrypoint"
    );
    assert!(
        patch_contract.contains("fn happy_path_is_restart_safe_and_reports_korean"),
        "patch lifecycle contract was not moved to its owner"
    );
}

#[test]
fn v03710_runtime_and_reporting_owners_hold_dispatch_and_output_decisions() {
    let korean_guard = "src/runtime_core/reporting/korean_guard.rs";
    let runtime_report = "src/runtime_core/reporting/runtime_report.rs";
    let runner = "src/runtime_core/workflow/application/runner.rs";
    for target in [korean_guard, runtime_report, runner] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.10 runtime owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod reporting;"),
        "reporting runtime owner is not crate-private"
    );
    let reporting_mod = fs::read_to_string("src/runtime_core/reporting/mod.rs").unwrap();
    for child in ["korean_guard", "runtime_report"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            reporting_mod.lines().any(|line| line == expected),
            "reporting child is not crate-private: {child}"
        );
    }
    let application_mod =
        fs::read_to_string("src/runtime_core/workflow/application/mod.rs").unwrap();
    assert!(
        application_mod
            .lines()
            .any(|line| line == "pub(crate) mod runner;"),
        "workflow application runner is not crate-private"
    );

    for (owner, rules) in [
        (
            korean_guard,
            [
                "struct StreamingGuard",
                "fn guard_or_failure",
                "fn validate",
            ]
            .as_slice(),
        ),
        (
            runtime_report,
            [
                "struct WorkflowResumeReport",
                "struct SessionResumeReport",
                "struct InitReport",
                "struct DoctorReport",
                "fn render_workflow_resume",
                "fn render_session_resume",
                "fn guard_patch_terminal",
                "fn render_init",
                "fn render_doctor",
            ]
            .as_slice(),
        ),
        (
            runner,
            [
                "trait RuntimeApplicationPort",
                "fn agent_run_report",
                "fn workflow_resume_report",
                "fn session_resume_report",
                "fn patch_approve_to_stdout",
                "fn patch_verify_report",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.10 owner is missing runtime rule: {owner} -> {rule}"
            );
        }
        for forbidden in [
            "crate::adapters",
            "crate::backend",
            "crate::context",
            "crate::intent",
            "crate::ledger",
            "crate::model",
            "crate::ontology",
            "crate::patch",
            "crate::state",
            "std::env",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(forbidden),
                "runtime owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    assert!(!Path::new("src/korean_guard.rs").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod korean_guard;"));

    let runtime_facade = fs::read_to_string("src/runtime.rs").unwrap();
    let production = runtime_facade
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or(&runtime_facade);
    for forbidden in [
        "fn guard_patch_terminal_report",
        "fn release_smoke_summary",
        "rpotato 진단\\n- CLI",
        "{}\\n- reconstructed context: {}",
    ] {
        assert!(
            !production.contains(forbidden),
            "legacy runtime facade retains moved report rule: {forbidden}"
        );
    }
    for delegation in [
        "impl RuntimeApplicationPort for LegacyRuntimeApplicationPort",
        "runner::workflow_resume_report",
        "runner::session_resume_report",
        "runner::patch_approve_to_stdout",
        "runner::patch_verify_report",
        "runtime_report::render_init",
        "runtime_report::render_doctor",
    ] {
        assert!(
            production.contains(delegation),
            "legacy runtime facade is missing owner delegation: {delegation}"
        );
    }
    assert!(
        runtime_facade.lines().count() <= 3_100,
        "runtime facade regrew beyond the v0.37.10 boundary"
    );
}

#[test]
fn v03711_extension_owners_hold_manifests_lifecycle_and_admission_policy() {
    let hook = "src/runtime_core/extensions/hook.rs";
    let skill = "src/runtime_core/extensions/skill.rs";
    let plugin = "src/runtime_core/extensions/plugin.rs";
    let hooks_adapter = "src/app/extensions_adapter/hooks.rs";
    let plugin_adapter = "src/app/extensions_adapter/plugin.rs";
    let skill_adapter = "src/app/extensions_adapter/skill.rs";
    for target in [hook, skill, plugin] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.11 extension owner: {target}"
        );
    }

    let extensions_mod = fs::read_to_string("src/runtime_core/extensions/mod.rs").unwrap();
    for child in ["hook", "skill", "plugin"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            extensions_mod.lines().any(|line| line == expected),
            "extension child is not crate-private: {child}"
        );
    }

    for (owner, rules, forbidden) in [
        (
            hook,
            [
                "enum HookStatus",
                "struct HookRule",
                "const HOOK_POINTS",
                "fn dispatch",
                "fn resolve_conflict",
            ]
            .as_slice(),
            [
                "crate::adapters",
                "crate::ledger",
                "crate::plugin",
                "crate::skill",
                "crate::state",
                "std::fs",
                "std::process",
            ]
            .as_slice(),
        ),
        (
            skill,
            [
                "struct SkillManifest",
                "enum ResolvedSkillManifest",
                "struct SkillRuntimeState",
                "fn validate_transition",
                "fn enforce_resolved_tool",
            ]
            .as_slice(),
            [
                "crate::adapters",
                "crate::hooks",
                "crate::plugin",
                "crate::state",
                "std::fs",
                "std::process",
            ]
            .as_slice(),
        ),
        (
            plugin,
            [
                "struct PluginCapability",
                "struct ParsedCodexSkill",
                "fn parse_codex_skill",
                "fn apply_manifest_risk_markers",
                "fn blocked_permissions",
            ]
            .as_slice(),
            [
                "crate::adapters",
                "crate::cli",
                "crate::ledger",
                "crate::state",
                "std::fs",
                "std::process",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.11 owner is missing extension rule: {owner} -> {rule}"
            );
        }
        for dependency in forbidden {
            assert!(
                !source.contains(dependency),
                "extension owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }

    for target in [hooks_adapter, plugin_adapter, skill_adapter] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.13 extension adapter: {target}"
        );
    }
    let adapter_mod = fs::read_to_string("src/app/extensions_adapter.rs").unwrap();
    for child in ["hooks", "plugin", "skill"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            adapter_mod.lines().any(|line| line == expected),
            "extension adapter child is not crate-private: {child}"
        );
    }

    for (adapter, moved_definition) in [
        (hooks_adapter, "enum HookStatus"),
        (hooks_adapter, "fn resolve_conflict"),
        (skill_adapter, "struct SkillManifest"),
        (skill_adapter, "fn validate_transition"),
        (plugin_adapter, "struct PluginCapability"),
        (plugin_adapter, "fn parse_codex_skill"),
        (plugin_adapter, "fn apply_manifest_risk_markers"),
    ] {
        let source = fs::read_to_string(adapter).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(moved_definition),
            "extension adapter retains moved rule: {adapter} -> {moved_definition}"
        );
    }

    for legacy in ["src/hooks.rs", "src/plugin.rs", "src/skill.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy extension root was restored: {legacy}"
        );
    }
    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_mod in ["mod hooks;", "mod plugin;", "mod skill;"] {
        assert!(
            !main.lines().any(|line| line == legacy_mod),
            "legacy extension root remains registered: {legacy_mod}"
        );
    }

    let hooks_adapter = fs::read_to_string(hooks_adapter).unwrap();
    let skill_adapter = fs::read_to_string(skill_adapter).unwrap();
    let plugin_adapter = fs::read_to_string(plugin_adapter).unwrap();
    assert!(
        hooks_adapter.lines().count() <= 300,
        "hooks adapter regrew beyond the v0.37.13 boundary"
    );
    assert!(
        skill_adapter.lines().count() <= 250,
        "skill adapter regrew beyond the v0.37.13 boundary"
    );
    assert!(
        plugin_adapter.lines().count() <= 1_750,
        "plugin adapter regrew beyond the v0.37.13 boundary"
    );
}

#[test]
fn v03712_collaboration_owners_hold_lifecycle_execution_and_reconciliation_policy() {
    let subagent_adapter = "src/app/collaboration_adapter/subagent.rs";
    let team_adapter = "src/app/collaboration_adapter/team.rs";
    let team_execution_adapter = "src/app/collaboration_adapter/team_execution.rs";
    let team_reconciliation_adapter = "src/app/collaboration_adapter/team_reconciliation.rs";
    let team_state_adapter = "src/app/collaboration_adapter/team_state.rs";
    let owners: &[(&str, &[&str])] = &[
        (
            "src/runtime_core/collaboration/subagent.rs",
            &[
                "enum SubagentRole",
                "enum SubagentStatus",
                "struct SubagentRecordV1",
                "fn validate_launch",
                "fn normalize_relative_path",
                "fn validate_record",
                "fn render_record",
            ],
        ),
        (
            "src/runtime_core/collaboration/subagent_result.rs",
            &[
                "struct SubagentResultV1",
                "fn parse_result_shape",
                "fn validate_patch_policy",
                "fn validate_context_binding",
                "fn verify_evidence_artifact",
                "fn render_evidence_payload_v2",
                "fn validate_bounded_text",
            ],
        ),
        (
            "src/runtime_core/collaboration/team.rs",
            &[
                "struct ContinuationDecision",
                "struct PolicyGate",
                "fn continuation_decision",
                "fn evaluate_policy_gate",
                "fn evaluate_ownership_gate",
                "fn dispatch_event_type",
                "fn admission_summary",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_execution.rs",
            &[
                "fn validate_execution_binding",
                "fn validate_execution_stage",
                "fn execution_mode",
                "fn validate_action_owner",
                "fn record_matches_team",
                "fn validate_completed_member_binding",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_reconciliation.rs",
            &[
                "fn validate_reconciliation_binding",
                "fn validate_reconciliation_stage",
                "fn validate_action_ownership",
                "fn validate_member_record",
                "fn render_reconciliation",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_state.rs",
            &[
                "enum TeamStage",
                "fn transition_to_at",
                "fn parse_manifest",
                "fn parse_state",
                "fn render_state",
            ],
        ),
    ];
    let collaboration_mod = fs::read_to_string("src/runtime_core/collaboration/mod.rs").unwrap();
    for (owner, rules) in owners {
        assert!(
            Path::new(owner).is_file(),
            "missing v0.37.12 collaboration owner: {owner}"
        );
        let child = Path::new(owner).file_stem().unwrap().to_str().unwrap();
        let expected = format!("pub(crate) mod {child};");
        assert!(
            collaboration_mod.lines().any(|line| line == expected),
            "collaboration child is not crate-private: {child}"
        );
        let source = fs::read_to_string(owner).unwrap();
        for rule in *rules {
            assert!(
                source.contains(rule),
                "v0.37.12 owner is missing collaboration rule: {owner} -> {rule}"
            );
        }
        for dependency in [
            "crate::adapters",
            "crate::backend",
            "crate::ledger",
            "crate::observability",
            "crate::state",
            "std::fs",
            "std::process",
            "std::thread",
        ] {
            assert!(
                !source.contains(dependency),
                "collaboration owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }

    for (facade, moved_definition) in [
        (subagent_adapter, "pub enum SubagentRole"),
        (subagent_adapter, "pub struct SubagentRecordV1"),
        (subagent_adapter, "fn validate_record"),
        (subagent_adapter, "fn render_record"),
        (subagent_adapter, "fn normalize_paths"),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "const RESULT_KEYS",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "const EVIDENCE_V2_KEYS",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn validate_patch",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn verify_evidence_artifact",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn render_evidence_payload_v2",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn validate_bounded_text",
        ),
        (team_adapter, "struct ContinuationDecision"),
        (team_adapter, "struct PolicyGate"),
        (team_adapter, "fn policy_preflight"),
        (team_adapter, "fn ownership_preflight"),
        (team_adapter, "fn admission_summary"),
        (team_execution_adapter, "fn pressure_from_status"),
        (team_execution_adapter, "fn record_matches_team"),
        (team_reconciliation_adapter, "fn validate_team_binding"),
        (team_reconciliation_adapter, "fn validate_member_record"),
        (team_state_adapter, "pub enum TeamStage"),
        (team_state_adapter, "fn parse_members"),
        (team_state_adapter, "fn render_state"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(moved_definition),
            "legacy collaboration facade retains moved rule: {facade} -> {moved_definition}"
        );
    }

    for (facade, delegation) in [
        (subagent_adapter, "collaboration::subagent::*"),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "result_policy::parse_result_shape",
        ),
        (team_adapter, "collaboration::team"),
        (team_execution_adapter, "validate_execution_binding"),
        (
            team_reconciliation_adapter,
            "validate_reconciliation_binding",
        ),
        (team_state_adapter, "collaboration::team_state"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            source.contains(delegation),
            "legacy collaboration facade is missing owner delegation: {facade} -> {delegation}"
        );
    }

    for (facade, maximum_lines) in [
        (subagent_adapter, 2_400),
        ("src/app/collaboration_adapter/subagent_result.rs", 800),
        (team_adapter, 1_400),
        (team_execution_adapter, 1_300),
        (team_reconciliation_adapter, 550),
        (team_state_adapter, 850),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            source.lines().count() <= maximum_lines,
            "collaboration facade regrew beyond the v0.37.12 boundary: {facade}"
        );
    }

    for legacy in [
        "src/subagent.rs",
        "src/team.rs",
        "src/team_execution.rs",
        "src/team_reconciliation.rs",
        "src/team_state.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy collaboration root was restored: {legacy}"
        );
    }
    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_mod in [
        "mod subagent;",
        "mod team;",
        "mod team_execution;",
        "mod team_reconciliation;",
        "mod team_state;",
        "pub mod team_state;",
    ] {
        assert!(
            !main.lines().any(|line| line == legacy_mod),
            "legacy collaboration root remains registered: {legacy_mod}"
        );
    }
    let adapter_mod = fs::read_to_string("src/app/collaboration_adapter.rs").unwrap();
    for child in [
        "subagent",
        "team",
        "team_execution",
        "team_reconciliation",
        "team_state",
    ] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            adapter_mod.lines().any(|line| line == expected),
            "collaboration adapter is not registered: {child}"
        );
    }

    assert_eq!(
        fs::read_to_string("tests/subagent_lifecycle.rs")
            .unwrap()
            .trim(),
        "include!(\"collaboration/subagent_lifecycle.rs\");"
    );
    assert_eq!(
        fs::read_to_string("tests/team_runtime.rs").unwrap().trim(),
        "include!(\"collaboration/team_runtime.rs\");"
    );
    assert!(!Path::new("src/subagent_result.rs").exists());
    assert!(Path::new("src/app/collaboration_adapter.rs").is_file());
    assert!(Path::new("src/app/collaboration_adapter/subagent_result.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod subagent_result;"));
}

#[test]
fn v03713_cli_surface_owners_replace_legacy_module() {
    let owner = fs::read_to_string("src/surfaces/cli/command.rs").unwrap();
    for definition in [
        "pub enum Command",
        "pub enum TeamCommand",
        "pub enum BackendCommand",
        "pub enum PluginCommand",
        "pub enum UninstallCommand",
    ] {
        assert!(
            owner.contains(definition),
            "CLI command owner is missing definition: {definition}"
        );
    }

    let parser_path = "src/surfaces/cli/parser.rs";
    let backend_parser_path = "src/surfaces/cli/parser/backend.rs";
    let collaboration_parser_path = "src/surfaces/cli/parser/collaboration.rs";
    let observability_parser_path = "src/surfaces/cli/parser/observability.rs";
    let patch_parser_path = "src/surfaces/cli/parser/patch.rs";
    let plugin_parser_path = "src/surfaces/cli/parser/plugin.rs";
    let parser_tests_path = "src/surfaces/cli/parser/tests/mod.rs";
    assert!(Path::new(backend_parser_path).is_file());
    assert!(Path::new(collaboration_parser_path).is_file());
    assert!(Path::new(observability_parser_path).is_file());
    assert!(Path::new(patch_parser_path).is_file());
    assert!(Path::new(plugin_parser_path).is_file());
    assert!(Path::new(parser_tests_path).is_file());
    let parser = fs::read_to_string(parser_path).unwrap();
    let backend_parser = fs::read_to_string(backend_parser_path).unwrap();
    let collaboration_parser = fs::read_to_string(collaboration_parser_path).unwrap();
    let observability_parser = fs::read_to_string(observability_parser_path).unwrap();
    let patch_parser = fs::read_to_string(patch_parser_path).unwrap();
    let plugin_parser = fs::read_to_string(plugin_parser_path).unwrap();
    let parser_tests = fs::read_to_string(parser_tests_path).unwrap();
    assert!(parser.contains("pub fn parse"));
    assert!(parser.contains("surfaces::cli::command::*"));
    assert!(
        parser.lines().any(|line| line == "mod backend;"),
        "CLI parser does not register the backend command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod collaboration;"),
        "CLI parser does not register the collaboration command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod observability;"),
        "CLI parser does not register the observability command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod patch;"),
        "CLI parser does not register the patch command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod plugin;"),
        "CLI parser does not register the plugin command-family owner"
    );
    for responsibility in [
        "pub(super) fn parse_backend_start(",
        "pub(super) fn parse_backend_chat(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "backend parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            backend_parser.contains(responsibility),
            "backend parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn parse_team_plan_args(",
        "pub(super) fn parse_team_admit_args(",
        "pub(super) fn parse_team_dispatch_args(",
        "pub(super) fn parse_team_governor_args(",
        "pub(super) fn parse_subagent_launch_args(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "collaboration parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            collaboration_parser.contains(responsibility),
            "collaboration parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in ["pub(super) fn parse_plugin_import("] {
        assert!(
            !parser.contains(responsibility),
            "plugin parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            plugin_parser.contains(responsibility),
            "plugin parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn parse_patch_preview(",
        "pub(super) fn parse_patch_approve(",
        "pub(super) fn parse_patch_verify(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "patch parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            patch_parser.contains(responsibility),
            "patch parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn parse_monitor_export(",
        "pub(super) fn parse_monitor_prune(",
        "pub(super) fn parse_ontology_context(",
        "pub(super) fn parse_ontology_import(",
        "pub(super) fn parse_benchmark_run(",
        "pub(super) fn parse_benchmark_report(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "observability parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            observability_parser.contains(responsibility),
            "observability parser is missing responsibility: {responsibility}"
        );
    }
    assert!(parser.contains("#[path = \"parser/tests/mod.rs\"]"));
    for responsibility in [
        "fn parses_subagent_launch_status_and_cancel(",
        "fn parses_backend_chat(",
        "fn parses_patch_approve_dry_run(",
        "fn parses_team_governor(",
        "fn parses_uninstall_dry_run_purge_cache(",
    ] {
        assert!(
            parser_tests.contains(responsibility),
            "CLI parser regression tests are missing responsibility: {responsibility}"
        );
    }
    assert!(
        parser.lines().count() < 590,
        "CLI parser production module regrew beyond its command-family extraction boundary"
    );
    assert!(
        backend_parser.lines().count() < 160,
        "backend parser regrew beyond its ownership boundary"
    );
    assert!(
        collaboration_parser.lines().count() < 550,
        "collaboration parser regrew beyond its ownership boundary"
    );
    assert!(
        observability_parser.lines().count() < 300,
        "observability parser regrew beyond its ownership boundary"
    );
    assert!(
        patch_parser.lines().count() < 170,
        "patch parser regrew beyond its ownership boundary"
    );
    assert!(
        plugin_parser.lines().count() < 80,
        "plugin parser regrew beyond its ownership boundary"
    );
    assert!(
        parser_tests.lines().count() < 1_500,
        "CLI parser regression module regrew beyond its ownership boundary"
    );

    let render = fs::read_to_string("src/surfaces/cli/render.rs").unwrap();
    assert!(render.contains("const HELP"));
    assert!(!parser.contains("const HELP"));

    assert!(
        !Path::new("src/cli.rs").exists(),
        "legacy CLI module remains after surface migration"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod cli;"));
}

#[test]
fn v03713_binary_entrypoint_delegates_process_outcome_to_startup() {
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("composition::startup::run"));
    assert!(!main.contains("eprintln!"));
    assert!(!main.contains("match app::run"));

    let startup = fs::read_to_string("src/composition/startup.rs").unwrap();
    assert!(startup.contains("korean_guard::guard_or_failure"));
    assert!(startup.contains("ExitCode::from(err.code)"));
}

#[test]
fn v03713_uninstall_plan_uses_composition_and_filesystem_owners() {
    let composition = fs::read_to_string("src/composition/uninstall.rs").unwrap();
    assert!(composition.contains("uninstall::managed_paths"));
    assert!(composition.contains("pub(crate) fn plan_report"));

    let adapter = fs::read_to_string("src/adapters/filesystem/uninstall.rs").unwrap();
    assert!(adapter.contains("struct ManagedUninstallPaths"));
    assert!(adapter.contains("pub(crate) fn managed_paths"));

    assert!(!Path::new("src/uninstall.rs").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod uninstall;"));
}

#[test]
fn v03713_unit_test_runtime_fixture_lives_under_test_support() {
    assert!(!Path::new("src/test_support.rs").exists());
    assert!(Path::new("tests/support/runtime_fixture.rs").is_file());

    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("#[path = \"../tests/support/runtime_fixture.rs\"]"));
    assert!(main.contains("mod test_support;"));
}

#[test]
fn v03713_tui_bridge_owns_read_and_selection_dtos() {
    let bridge = fs::read_to_string("src/surfaces/tui/runtime_bridge.rs").unwrap();
    for definition in [
        "struct TuiReadBudget",
        "enum TuiReadRequest",
        "struct TuiReadPage",
        "struct SelectionLease",
        "struct SelectionObservation",
        "enum TuiFreshness",
        "enum TuiIntent",
        "struct OneShotSecret",
        "fn new_tui_intent_id",
        "fn lease_matches_active_workflow",
        "fn lease_matches_terminal_selection",
    ] {
        assert!(
            bridge.contains(definition),
            "TUI runtime bridge is missing {definition}"
        );
    }

    let outcome = fs::read_to_string("src/surfaces/tui/outcome.rs").unwrap();
    for definition in [
        "enum TuiOutcomeCode",
        "struct TuiOutcome",
        "fn exact_tui_outcome",
        "fn unsupported_source_platform_outcome",
        "fn validate_tui_id",
    ] {
        assert!(
            outcome.contains(definition),
            "TUI outcome owner is missing {definition}"
        );
    }

    let runtime = fs::read_to_string("src/runtime.rs").unwrap();
    assert!(!runtime.contains("pub struct TuiReadBudget"));
    assert!(!runtime.contains("pub struct SelectionLease"));
    assert!(!runtime.contains("pub enum TuiIntent"));
    assert!(!runtime.contains("pub struct OneShotSecret"));
    assert!(!runtime.contains("pub enum TuiOutcomeCode"));
    assert!(!runtime.contains("pub struct TuiOutcome"));
    assert!(!runtime.contains("pub(crate) fn exact_tui_outcome"));
    assert!(!runtime.contains("fn unsupported_source_platform_outcome"));
    assert!(!runtime.contains("fn new_tui_intent_id"));
    assert!(!runtime.contains("fn tui_lease_matches_workflow_under_transition"));
    assert!(!runtime.contains("fn tui_lease_matches_terminal_selection_under_transition"));
    assert!(!runtime.contains("fn validate_tui_id"));
    assert!(!runtime.contains("fn tui_selection_lease"));
    assert!(!runtime.contains("fn tui_gate_descriptor"));
    assert!(!runtime.contains("fn dispatch_tui_intent"));

    for legacy_owner in [
        "src/patch.rs",
        "src/app/workflow_adapter/state.rs",
        "src/tui.rs",
    ] {
        let source = fs::read_to_string(legacy_owner).unwrap();
        for facade_type in [
            "crate::runtime::SelectionLease",
            "crate::runtime::TuiGateKind",
        ] {
            assert!(
                !source.contains(facade_type),
                "{legacy_owner} still imports TUI contract through {facade_type}"
            );
        }
    }

    let tui_read = fs::read_to_string("src/composition/tui_read.rs").unwrap();
    assert!(tui_read.contains("fn read_tui_page"));
    assert!(tui_read.contains("trait TuiReadPort"));
    assert!(tui_read.contains("port.state_snapshot"));
    assert!(!runtime.contains("fn read_tui_page"));

    let tui_action = fs::read_to_string("src/composition/tui_action.rs").unwrap();
    for definition in [
        "trait TuiActionPort",
        "enum TuiMutationFailure",
        "fn selection_lease",
        "fn gate_descriptor",
        "fn dispatch_intent",
    ] {
        assert!(
            tui_action.contains(definition),
            "TUI action owner is missing {definition}"
        );
    }

    let page = fs::read_to_string("src/surfaces/tui/page.rs").unwrap();
    for definition in [
        "fn bounded_budget_for",
        "fn page_slice",
        "fn paged_chars",
        "fn paged_diff",
        "fn page_has_next",
        "fn page_continuation",
        "fn state_page_authority",
        "fn unavailable_page",
        "fn build_page",
    ] {
        assert!(
            page.contains(definition),
            "TUI page owner is missing {definition}"
        );
        assert!(
            !runtime.contains(definition),
            "legacy runtime still owns {definition}"
        );
    }

    let view_model = fs::read_to_string("src/surfaces/tui/view_model.rs").unwrap();
    for definition in [
        "enum InteractiveView",
        "struct InteractiveState",
        "struct EvidenceReportView",
        "struct SessionsReportView",
        "struct SessionSummaryView",
        "struct OverviewReportView",
        "struct MonitorReportView",
        "struct TranscriptReportView",
        "fn set_view",
        "fn read_request",
    ] {
        assert!(
            view_model.contains(definition),
            "TUI view-model owner is missing {definition}"
        );
    }
    let legacy_tui = fs::read_to_string("src/tui.rs").unwrap();
    assert!(legacy_tui.contains("surfaces::tui::view_model"));
    assert!(legacy_tui.contains("impl TuiActionPort for LegacyTuiActionPort"));
    assert!(legacy_tui.contains("impl TuiReadPort for LegacyTuiReadPort"));
    assert!(!legacy_tui.contains("enum InteractiveView"));
    assert!(!legacy_tui.contains("struct InteractiveState"));

    let controller = fs::read_to_string("src/surfaces/tui/controller.rs").unwrap();
    for definition in [
        "trait TuiRuntimePort",
        "fn run_controller",
        "fn terminal_fault_error",
        "fn consume_outcome",
    ] {
        assert!(
            controller.contains(definition),
            "TUI controller owner is missing {definition}"
        );
    }
    assert!(!controller.contains("use crate::runtime;"));
    assert!(!controller.contains("crate::runtime::"));
    assert!(!controller.contains("crate::adapters"));
    assert!(legacy_tui.contains("impl TuiRuntimePort for LegacyTuiRuntimePort"));

    let terminal_port = fs::read_to_string("src/runtime_core/terminal.rs").unwrap();
    for definition in [
        "enum TerminalFault",
        "enum FrameWriteBoundary",
        "trait TerminalIo",
    ] {
        assert!(
            terminal_port.contains(definition),
            "terminal contract owner is missing {definition}"
        );
    }
    let native_terminal = fs::read_to_string("src/adapters/terminal/native.rs").unwrap();
    assert!(native_terminal.contains("runtime_core::terminal"));
    assert!(!native_terminal.contains("pub enum TerminalFault"));
    assert!(!native_terminal.contains("pub trait TerminalIo"));

    let render = fs::read_to_string("src/surfaces/tui/render.rs").unwrap();
    for definition in [
        "fn render_interactive_frame",
        "fn render_notice_lines",
        "fn sanitize_terminal_text",
        "fn truncate_chars",
        "fn terminal_width",
        "fn push_wrapped",
        "fn bytes_label",
    ] {
        assert!(
            render.contains(definition),
            "TUI interactive render owner is missing {definition}"
        );
        assert!(
            !legacy_tui.contains(definition),
            "legacy TUI still owns {definition}"
        );
    }

    let report_render = fs::read_to_string("src/surfaces/tui/report_render.rs").unwrap();
    for definition in [
        "fn canonical_page_report",
        "fn authority_pair",
        "fn render_evidence_report",
        "fn render_sessions_report",
        "fn render_overview_report",
        "fn render_monitor_report",
        "fn render_transcript_report",
    ] {
        assert!(
            report_render.contains(definition),
            "TUI report render owner is missing {definition}"
        );
        assert!(
            !legacy_tui.contains(definition),
            "legacy TUI still owns {definition}"
        );
    }
}

#[test]
fn v03713_composition_owns_cli_preflight_and_dispatch_ordering() {
    let composition = fs::read_to_string("src/composition/dispatch.rs").unwrap();
    for definition in [
        "trait CommandDispatchPort",
        "fn run(",
        "parser::parse(args)",
        "port.validate_native_terminal()",
        "port.recover_pending_source_bundles()",
        "port.execute(command)",
    ] {
        assert!(
            composition.contains(definition),
            "CLI composition owner is missing {definition}"
        );
    }

    let app = fs::read_to_string("src/app.rs").unwrap();
    assert!(app.contains("dispatch::run(args"));
    assert!(!app.contains("parser::parse(args)"));
    assert!(!app.contains("recover_pending_source_bundles()?"));
    assert!(!app.contains("match command"));

    let adapter = fs::read_to_string("src/app/legacy_dispatch.rs").unwrap();
    assert!(adapter.contains("impl dispatch::CommandDispatchPort for LegacyCommandDispatchPort"));
    assert!(adapter.contains("match command"));
}

#[test]
fn v03713_composition_owns_benchmark_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait BenchmarkCommandPort",
        "fn run_benchmark(",
        "BenchmarkCommand::Validate",
        "BenchmarkCommand::Record",
        "BenchmarkCommand::Run",
        "BenchmarkCommand::Report",
        "CommandOutput::Exact",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::benchmark", "crate::ledger", "crate::observability"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its benchmark port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/legacy_dispatch.rs").unwrap();
    assert!(adapter.contains("impl inference::BenchmarkCommandPort"));
    assert!(adapter.contains("inference::run_benchmark(command, self)"));

    assert!(!Path::new("src/benchmark.rs").exists());
    assert!(Path::new("src/app/inference_adapter/benchmark.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod benchmark;"));
}

#[test]
fn v03713_composition_owns_model_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait ModelCommandPort",
        "fn run_model(",
        "ModelCommand::List",
        "ModelCommand::Manifest",
        "ModelCommand::Inspect",
        "ModelCommand::SetDefault",
        "ModelCommand::FetchCandidate",
        "ModelCommand::Promote",
        "ModelCommand::Install",
        "CommandOutput::None",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::model", "crate::ledger", "crate::observability"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its model port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/legacy_dispatch.rs").unwrap();
    assert!(adapter.contains("impl inference::ModelCommandPort"));
    assert!(adapter.contains("inference::run_model(command, self)"));

    assert!(!Path::new("src/model.rs").exists());
    assert!(Path::new("src/app/inference_adapter/model.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod model;"));
}

#[test]
fn v03713_composition_owns_backend_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait BackendCommandPort",
        "fn run_backend(",
        "BackendCommand::Doctor",
        "BackendCommand::Install",
        "BackendCommand::Start",
        "port.default_model_path()",
        "BackendCommand::VerifyArchive",
        "BackendCommand::Chat",
        "port.chat_stream_report",
        "port.chat_report",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::backend", "crate::model", "crate::ledger"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its backend port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/legacy_dispatch.rs").unwrap();
    assert!(adapter.contains("impl inference::BackendCommandPort"));
    assert!(adapter.contains("inference::run_backend(command, self, &mut writer)"));

    assert!(!Path::new("src/backend.rs").exists());
    assert!(Path::new("src/app/inference_adapter/backend.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod backend;"));
}

#[test]
fn v03713_platform_fixtures_are_grouped_under_support_boundary() {
    for name in [
        "fake_sidecar.rs",
        "native_terminal.rs",
        "native_terminal_probe.rs",
    ] {
        assert!(!Path::new(&format!("tests/support/{name}")).exists());
        assert!(Path::new(&format!("tests/support/platform/{name}")).is_file());
    }

    let harness = fs::read_to_string("tests/surfaces.rs").unwrap();
    assert!(harness.contains("support/platform/native_terminal.rs"));
    assert!(harness.contains("surfaces/interactive_tui.rs"));
    assert!(harness.contains("surfaces/native_terminal.rs"));
    assert!(!Path::new("tests/platform.rs").exists());
    assert!(!Path::new("tests/platform").exists());
}

#[test]
fn v03713_state_adapter_separates_persistence_responsibilities() {
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let current_snapshot_adapter = "src/app/workflow_adapter/state/current_snapshot.rs";
    let current_transition_adapter = "src/app/workflow_adapter/state/current_transition.rs";
    let lifecycle_adapter = "src/app/workflow_adapter/state/lifecycle.rs";
    let source_install_adapter = "src/app/workflow_adapter/state/source_install.rs";
    let transaction_adapter = "src/app/workflow_adapter/state/transaction.rs";
    let transition_commit_adapter = "src/app/workflow_adapter/state/transition_commit.rs";
    let workflow_revision_adapter = "src/app/workflow_adapter/state/workflow_revision.rs";
    let workflow_store_adapter = "src/app/workflow_adapter/state/workflow_store.rs";
    let state_test_modules = [
        "src/app/workflow_adapter/state/tests/mod.rs",
        "src/app/workflow_adapter/state/tests/callgraph.rs",
        "src/app/workflow_adapter/state/tests/current_snapshot.rs",
        "src/app/workflow_adapter/state/tests/lifecycle.rs",
        "src/app/workflow_adapter/state/tests/source_install.rs",
        "src/app/workflow_adapter/state/tests/workflow_store.rs",
    ];
    assert!(Path::new(state_adapter).is_file());
    assert!(Path::new(current_snapshot_adapter).is_file());
    assert!(Path::new(current_transition_adapter).is_file());
    assert!(Path::new(lifecycle_adapter).is_file());
    assert!(Path::new(source_install_adapter).is_file());
    assert!(Path::new(transaction_adapter).is_file());
    assert!(Path::new(transition_commit_adapter).is_file());
    assert!(Path::new(workflow_revision_adapter).is_file());
    assert!(Path::new(workflow_store_adapter).is_file());
    for test_module in state_test_modules {
        assert!(Path::new(test_module).is_file());
    }

    let state = fs::read_to_string(state_adapter).unwrap();
    assert!(state.lines().any(|line| line == "mod current_snapshot;"));
    assert!(state.lines().any(|line| line == "mod current_transition;"));
    assert!(state.lines().any(|line| line == "mod lifecycle;"));
    assert!(state.lines().any(|line| line == "mod source_install;"));
    assert!(state.lines().any(|line| line == "mod transaction;"));
    assert!(state.lines().any(|line| line == "mod transition_commit;"));
    assert!(state.lines().any(|line| line == "mod workflow_revision;"));
    assert!(state.lines().any(|line| line == "mod workflow_store;"));
    assert!(state.contains("#[path = \"state/tests/mod.rs\"]"));
    assert!(!state.contains("mod tests {"));
    for escaped_responsibility in [
        "fn parse_current_state(",
        "fn promote_current_state_v1(",
        "struct StateTransitionRecoveryPort",
        "struct StateTransitionTransactionAdapter",
        "fn validate_prepared_state_current_member(",
        "struct StateReconcileTransactionPort",
        "fn reconcile_invalid_current_under_guard(",
        "fn decode_prepared_current_image(",
        "pub fn session_resume_report(",
        "pub fn reconcile_report(",
        "struct PreparedSourceDir",
        "fn recover_source_replace",
        "struct StateApprovalTransactionPort",
        "struct StateVerificationTransactionPort",
        "struct WorkflowCheckpointGuard",
        "fn build_prepared_workflow_revision(",
        "struct StateWorkflowRecoveryPort",
        "fn validate_workflow_chain(",
    ] {
        assert!(
            !state.contains(escaped_responsibility),
            "state child responsibility escaped into parent adapter: {escaped_responsibility}"
        );
    }

    let current_snapshot = fs::read_to_string(current_snapshot_adapter).unwrap();
    for owned_responsibility in [
        "fn parse_current_state(",
        "fn promote_current_state_v1(",
        "fn render_current_state_v2(",
    ] {
        assert!(
            current_snapshot.contains(owned_responsibility),
            "current snapshot adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let current_transition = fs::read_to_string(current_transition_adapter).unwrap();
    for owned_responsibility in [
        "struct StateTransitionRecoveryPort",
        "struct StateTransitionTransactionAdapter",
        "fn validate_prepared_state_current_member(",
    ] {
        assert!(
            current_transition.contains(owned_responsibility),
            "current transition adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let lifecycle = fs::read_to_string(lifecycle_adapter).unwrap();
    for owned_responsibility in [
        "pub fn initialize(",
        "pub fn reconcile_report(",
        "pub fn session_resume_report(",
    ] {
        assert!(
            lifecycle.contains(owned_responsibility),
            "state lifecycle adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let source_install = fs::read_to_string(source_install_adapter).unwrap();
    for owned_responsibility in [
        "struct PreparedSourceDir",
        "struct PreparedRollbackDir",
        "fn recover_source_replace",
    ] {
        assert!(
            source_install.contains(owned_responsibility),
            "source installation adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let workflow_store = fs::read_to_string(workflow_store_adapter).unwrap();
    for owned_responsibility in [
        "struct StateWorkflowRecoveryPort",
        "fn validate_workflow_chain(",
        "fn write_workflow_snapshot_bytes(",
    ] {
        assert!(
            workflow_store.contains(owned_responsibility),
            "workflow store adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let workflow_revision = fs::read_to_string(workflow_revision_adapter).unwrap();
    for owned_responsibility in [
        "struct WorkflowCheckpointGuard",
        "fn build_prepared_workflow_revision(",
        "fn decode_prepared_workflow_revision(",
    ] {
        assert!(
            workflow_revision.contains(owned_responsibility),
            "workflow revision adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let transaction = fs::read_to_string(transaction_adapter).unwrap();
    for owned_responsibility in [
        "struct StateApprovalTransactionPort",
        "struct StateVerificationTransactionPort",
        "struct StateTerminalActionTransactionPort",
    ] {
        assert!(
            transaction.contains(owned_responsibility),
            "state transaction adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let transition_commit = fs::read_to_string(transition_commit_adapter).unwrap();
    for owned_responsibility in [
        "struct StateReconcileTransactionPort",
        "fn reconcile_invalid_current_under_guard(",
        "fn decode_prepared_current_image(",
    ] {
        assert!(
            transition_commit.contains(owned_responsibility),
            "state transition commit adapter is missing responsibility: {owned_responsibility}"
        );
    }

    assert!(state.lines().count() < 1_000);
    assert!(current_snapshot.lines().count() < 1_000);
    assert!(current_transition.lines().count() < 700);
    assert!(lifecycle.lines().count() < 700);
    assert!(source_install.lines().count() < 1_000);
    assert!(transaction.lines().count() < 700);
    assert!(transition_commit.lines().count() < 450);
    assert!(workflow_revision.lines().count() < 500);
    assert!(workflow_store.lines().count() < 500);
    for test_module in state_test_modules {
        let tests = fs::read_to_string(test_module).unwrap();
        assert!(
            tests.lines().count() < 700,
            "oversized state test module: {test_module}"
        );
    }
}

fn dependency_edges(root: &Object) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let contract = field_object(root, "dependency_contract", "map");
    let roots = string_array(
        field_array(contract, "roots", "map.dependency_contract"),
        "map.dependency_contract.roots",
    )
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut edges = BTreeSet::new();
    for (index, value) in field_array(contract, "allowed_edges", "map.dependency_contract")
        .iter()
        .enumerate()
    {
        let context = format!("map.dependency_contract.allowed_edges[{index}]");
        let edge = as_object(value, &context);
        edges.insert((
            field_string(edge, "from", &context).to_owned(),
            field_string(edge, "to", &context).to_owned(),
        ));
    }
    assert!(
        field_array(contract, "exceptions", "map.dependency_contract").is_empty(),
        "v0.37.1 dependency contract must not begin with exceptions"
    );
    (roots, edges)
}

fn direct_dependencies() -> BTreeSet<String> {
    let cargo = fs::read_to_string("Cargo.toml").expect("Cargo.toml must be readable");
    let mut in_dependencies = false;
    let mut dependencies = BTreeSet::new();
    for line in cargo.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]";
            continue;
        }
        if in_dependencies && !line.is_empty() && !line.starts_with('#') {
            let name = line
                .split_once('=')
                .map(|(name, _)| name.trim())
                .unwrap_or_else(|| panic!("invalid dependency declaration: {line}"));
            dependencies.insert(name.to_owned());
        }
    }
    dependencies
}

#[test]
fn dependency_contract_rejects_forbidden_imports_and_new_parser_crates() {
    let map = load_map();
    let root = as_object(&map, "map");
    let (roots, edges) = dependency_edges(root);
    assert_eq!(
        roots,
        ARCHITECTURE_ROOTS.into_iter().map(str::to_owned).collect()
    );
    let required_edges = BTreeSet::from([
        ("composition".to_owned(), "surfaces".to_owned()),
        ("composition".to_owned(), "runtime_core".to_owned()),
        ("composition".to_owned(), "adapters".to_owned()),
        ("composition".to_owned(), "foundation".to_owned()),
        ("surfaces".to_owned(), "runtime_core".to_owned()),
        ("surfaces".to_owned(), "foundation".to_owned()),
        ("runtime_core".to_owned(), "foundation".to_owned()),
        ("adapters".to_owned(), "runtime_core".to_owned()),
        ("adapters".to_owned(), "foundation".to_owned()),
    ]);
    assert_eq!(
        edges, required_edges,
        "dependency contract was weakened or widened"
    );

    for source_root in &roots {
        for path in collect_rust_files(&format!("src/{source_root}")) {
            let source = fs::read_to_string(&path).unwrap();
            for (line_index, line) in source.lines().enumerate() {
                let line = line.trim_start();
                let Some(import) = line
                    .strip_prefix("use crate::")
                    .or_else(|| line.strip_prefix("pub(crate) use crate::"))
                else {
                    continue;
                };
                let target_root = import.split([':', ';', '{']).next().unwrap_or("");
                assert!(
                    roots.contains(target_root),
                    "{path}:{} imports concrete legacy root {target_root}",
                    line_index + 1
                );
                assert!(
                    source_root == target_root
                        || edges.contains(&(source_root.clone(), target_root.to_owned())),
                    "{path}:{} has forbidden dependency {source_root} -> {target_root}",
                    line_index + 1
                );
            }
        }
    }

    assert_eq!(
        direct_dependencies(),
        BTreeSet::from([
            "flate2".to_owned(),
            "rusqlite".to_owned(),
            "sha2".to_owned(),
            "tar".to_owned(),
            "ureq".to_owned(),
            "zip".to_owned(),
        ]),
        "v0.37.1 must not add a parser or architecture-test dependency"
    );
}
