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
const ARCHITECTURE_ROOTS: [&str; 6] = [
    "app",
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

fn collect_rust_files(root: &str) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    collect_recursive(
        Path::new(root),
        &BTreeSet::from(["rs".to_owned()]),
        &mut files,
    );
    files
}

#[path = "architecture_contract/adapter_boundaries.rs"]
mod adapter_boundaries;
#[path = "architecture_contract/cli_install.rs"]
mod cli_install;
#[path = "architecture_contract/collaboration.rs"]
mod collaboration;
#[path = "architecture_contract/composition.rs"]
mod composition;
#[path = "architecture_contract/domain_views.rs"]
mod domain_views;
#[path = "architecture_contract/foundation.rs"]
mod foundation_contract;
#[path = "architecture_contract/inference.rs"]
mod inference;
#[path = "architecture_contract/migration_map.rs"]
mod migration_map;
#[path = "architecture_contract/observability_policy.rs"]
mod observability_policy;
#[path = "architecture_contract/patch_runtime.rs"]
mod patch_runtime;
#[path = "architecture_contract/session_web_browser.rs"]
mod session_web_browser;
#[path = "architecture_contract/tui_bridge.rs"]
mod tui_bridge;
#[path = "architecture_contract/tui_extension.rs"]
mod tui_extension;
#[path = "architecture_contract/workflow_transition.rs"]
mod workflow_transition;
