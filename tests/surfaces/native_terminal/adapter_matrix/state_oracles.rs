pub(super) fn assert_tree_unchanged(
    before: &std::collections::BTreeMap<String, Vec<u8>>,
    after: &std::collections::BTreeMap<String, Vec<u8>>,
    context: &str,
) {
    let changed = before
        .keys()
        .chain(after.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|path| !is_ignorable_entry_metadata(path) && before.get(*path) != after.get(*path))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "{context} must have zero product-state delta (coordination locks and the bounded latest-release cache excluded); changed paths: {changed:?}"
    );
}

fn is_ignorable_entry_metadata(path: &str) -> bool {
    path.ends_with(".lock") || path.replace('\\', "/").ends_with("/cache/update-latest-v2")
}

#[test]
fn zero_delta_entry_excludes_only_the_bounded_latest_release_cache() {
    assert!(is_ignorable_entry_metadata("1/cache/update-latest-v2"));
    assert!(is_ignorable_entry_metadata("1\\cache\\update-latest-v2"));
    assert!(!is_ignorable_entry_metadata(
        "1/cache/updates/v0.44.0/rpotato.ready"
    ));
    assert!(!is_ignorable_entry_metadata("1/state/current-state.json"));
}

fn runtime_ledger(fixture: &NativeTerminalFixture) -> String {
    std::fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap()
}

fn event_delta(before: &str, after: &str, event_type: &str) -> usize {
    let needle = format!("\"event_type\":\"{event_type}\"");
    after.matches(&needle).count() - before.matches(&needle).count()
}

#[cfg(unix)]
fn assert_unix_approval_oracle(
    fixture: &NativeTerminalFixture,
    pending: &native_terminal_support::PendingSourceApproval,
    before_ledger: &str,
    before_workflow_revision: u64,
    before_current_revision: u64,
) {
    assert_eq!(
        std::fs::read_to_string(&pending.source).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let ledger = runtime_ledger(fixture);
    let before_count = before_ledger.lines().count();
    let lines = ledger.lines().collect::<Vec<_>>();
    let committed = &lines[before_count..];
    let expected_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    assert_eq!(committed.len(), expected_types.len(), "exact E0..E9 count");
    let mut ids = std::collections::BTreeSet::new();
    let mut previous = before_ledger
        .lines()
        .last()
        .map(|line| json_string(line, "event_hash"))
        .unwrap_or_else(|| "root".to_string());
    for (index, (line, expected_type)) in committed.iter().zip(expected_types).enumerate() {
        assert_eq!(
            json_string(line, "event_type"),
            expected_type,
            "E{index} event type"
        );
        assert_eq!(
            json_string(line, "previous_event_hash"),
            previous,
            "E{index} predecessor hash"
        );
        assert!(
            ids.insert(json_string(line, "event_id")),
            "E{index} event id must be unique"
        );
        previous = json_string(line, "event_hash");
    }
    for (event_type, expected) in [
        ("runtime.intent.accepted", 1),
        ("patch.apply.approved", 1),
        ("patch.applied", 1),
        ("transcript.recorded", 1),
        ("workflow.checkpoint", 2),
        ("hook.dispatched", 4),
    ] {
        assert_eq!(event_delta(before_ledger, &ledger, event_type), expected);
    }
    let pointer = std::fs::read_to_string(
        fixture
            .project
            .join(".rpotato/workflows")
            .join(format!("{}.json", pending.workflow_id)),
    )
    .unwrap();
    assert_eq!(
        json_u64(&pointer, "committed_revision"),
        before_workflow_revision + 2
    );
    let current =
        std::fs::read_to_string(fixture.project.join(".rpotato/state/current-state.json")).unwrap();
    assert_eq!(json_u64(&current, "revision"), before_current_revision + 1);
    assert_eq!(
        json_u64(&current, "event_count"),
        u64::try_from(before_count + 10).unwrap()
    );
    assert_eq!(
        json_string(&current, "event_id"),
        json_string(committed[9], "event_id")
    );
    assert_eq!(json_string(&current, "event_hash"), previous);
    let head =
        std::fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl.head")).unwrap();
    assert_eq!(
        json_u64(&head, "event_count"),
        u64::try_from(lines.len()).unwrap()
    );
    assert_eq!(json_string(&head, "last_event_hash"), previous);
    assert_eq!(
        std::fs::read_to_string(fixture.project.join(".rpotato/session-ledger.jsonl")).unwrap(),
        ledger,
        "T10 project ledger must exactly converge to runtime authority"
    );
    assert_eq!(
        std::fs::read(fixture.data.join("logs/operation.log")).unwrap(),
        expected_operation_log_bytes(&lines),
        "T10 operation log must exactly converge to runtime authority"
    );
    let projected = {
        let connection =
            rusqlite::Connection::open(fixture.data.join("state/observability.sqlite")).unwrap();
        let mut statement = connection
            .prepare(
                "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
                   FROM ledger_events
               ORDER BY rowid",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        projected,
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                (
                    i64::try_from(index + 1).unwrap(),
                    json_string(line, "event_id"),
                    i64::try_from(json_u64(line, "ts_ms")).unwrap(),
                    json_string(line, "event_type"),
                    json_string(line, "project_id"),
                    json_string(line, "session_id"),
                    json_string(line, "summary"),
                )
            })
            .collect::<Vec<_>>(),
        "T10 sqlite rows and ordinals must exactly converge to runtime authority"
    );
    assert_directory_has_no_suffix(
        &fixture.project.join(".rpotato/transition-journal"),
        ".prepared.json",
    );
    assert_directory_has_no_suffix(&fixture.data.join("state/projection-lag"), ".json");
}

#[cfg(unix)]
fn expected_operation_log_bytes(lines: &[&str]) -> Vec<u8> {
    let mut output = lines
        .iter()
        .map(|line| {
            format!(
                "{} {} {} {}",
                json_u64(line, "ts_ms"),
                json_string(line, "event_type"),
                json_string(line, "session_id"),
                json_string(line, "summary")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    if !output.is_empty() {
        output.push(b'\n');
    }
    output
}

#[cfg(unix)]
fn assert_directory_has_no_suffix(path: &std::path::Path, suffix: &str) {
    if !path.exists() {
        return;
    }
    let mut pending = vec![path.to_path_buf()];
    let mut matches = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap().flatten() {
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(suffix))
            {
                matches.push(entry.path());
            }
        }
    }
    assert!(
        matches.is_empty(),
        "unexpected durable residue: {matches:?}"
    );
}

fn tree_contains(root: &std::path::Path, needle: &[u8]) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            tree_contains(&path, needle)
        } else {
            std::fs::read(path)
                .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
                .unwrap_or(false)
        }
    })
}

#[cfg(unix)]
fn json_u64(body: &str, key: &str) -> u64 {
    body.split(&format!("\"{key}\":"))
        .nth(1)
        .and_then(|tail| {
            let digits = tail
                .trim_start()
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>();
            digits.parse().ok()
        })
        .unwrap_or_else(|| panic!("missing numeric JSON field: {key}"))
}

#[cfg(unix)]
fn json_string(body: &str, key: &str) -> String {
    body.split(&format!("\"{key}\":"))
        .nth(1)
        .map(str::trim_start)
        .and_then(|tail| tail.strip_prefix('"'))
        .and_then(|tail| tail.split('"').next())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("missing string JSON field: {key}"))
        .to_string()
}
