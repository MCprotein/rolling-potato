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

fn dependency_root_roles(root: &Object) -> BTreeMap<String, String> {
    let contract = field_object(root, "dependency_contract", "map");
    field_array(contract, "root_roles", "map.dependency_contract")
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("map.dependency_contract.root_roles[{index}]");
            let role = as_object(value, &context);
            (
                field_string(role, "root", &context).to_owned(),
                field_string(role, "role", &context).to_owned(),
            )
        })
        .collect()
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
    let root_roles = dependency_root_roles(root);
    assert_eq!(
        roots,
        ARCHITECTURE_ROOTS.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(
        root_roles,
        BTreeMap::from([
            (
                "adapters".to_owned(),
                "filesystem, process, database, network, and terminal infrastructure".to_owned(),
            ),
            (
                "app".to_owned(),
                "executable integration shell and inbound adapter wiring".to_owned(),
            ),
            (
                "composition".to_owned(),
                "cross-capability orchestration and command use cases".to_owned(),
            ),
            (
                "foundation".to_owned(),
                "dependency-free shared errors and integrity primitives".to_owned(),
            ),
            (
                "runtime_core".to_owned(),
                "I/O-independent domain policy, state machines, and ports".to_owned(),
            ),
            (
                "surfaces".to_owned(),
                "CLI and TUI presentation, input, and controller drivers".to_owned(),
            ),
        ]),
        "physical architecture roots must retain one explicit role each"
    );
    assert_eq!(
        root_roles.keys().cloned().collect::<BTreeSet<_>>(),
        roots,
        "every architecture root must have exactly one role"
    );
    let required_edges = BTreeSet::from([
        ("app".to_owned(), "composition".to_owned()),
        ("app".to_owned(), "surfaces".to_owned()),
        ("app".to_owned(), "runtime_core".to_owned()),
        ("app".to_owned(), "adapters".to_owned()),
        ("app".to_owned(), "foundation".to_owned()),
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
