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
        "tests/platform.rs",
        "tests/platform/interactive_tui.rs",
        "tests/platform/native_terminal.rs",
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
