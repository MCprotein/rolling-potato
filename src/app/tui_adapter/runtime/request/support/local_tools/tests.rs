use super::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rpotato-local-tools-{label}-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(path).unwrap()
}

fn call(id: AgentToolId, input: impl Into<String>) -> LocalAgentToolCall {
    LocalAgentToolCall {
        id,
        input: input.into(),
    }
}

fn run(root: &Path, id: AgentToolId, input: impl Into<String>) -> ToolObservation {
    LocalToolExecutor {
        root: root.to_path_buf(),
        commands: command::CommandPaths::resolve(root),
    }
    .execute(
        &call(id, input),
        &RequestCancellationToken::default(),
        Duration::from_secs(2),
    )
}

fn init_git(root: &Path) {
    let commands = command::CommandPaths::resolve(root);
    let git = commands.path_for("git").expect("git executable");
    let status = std::process::Command::new(git)
        .args(["init", "--quiet"])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", command::null_device())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn read_file_rejects_traversal_symlinks_binary_and_invalid_utf8() {
    let root = fixture("read-denied");
    fs::write(root.join("binary"), b"a\0b").unwrap();
    fs::write(root.join("invalid"), [0xff]).unwrap();
    assert_eq!(
        run(&root, AgentToolId::ReadFile, "../outside").status,
        ToolObservationStatus::Denied
    );
    assert_eq!(
        run(&root, AgentToolId::ReadFile, "binary").status,
        ToolObservationStatus::Denied
    );
    assert_eq!(
        run(&root, AgentToolId::ReadFile, "invalid").status,
        ToolObservationStatus::Denied
    );
    #[cfg(unix)]
    {
        fs::create_dir(root.join("real")).unwrap();
        fs::write(root.join("real/file"), "inside").unwrap();
        std::os::unix::fs::symlink("binary", root.join("link")).unwrap();
        std::os::unix::fs::symlink("real", root.join("parent-link")).unwrap();
        assert_eq!(
            run(&root, AgentToolId::ReadFile, "link").status,
            ToolObservationStatus::Denied
        );
        assert_eq!(
            run(&root, AgentToolId::ReadFile, "parent-link/file").status,
            ToolObservationStatus::Denied
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_file_caps_lines_and_bytes_on_utf8_boundaries() {
    let root = fixture("read-caps");
    fs::write(root.join("lines"), "x\n".repeat(401)).unwrap();
    fs::write(root.join("utf8"), "가".repeat(6_000)).unwrap();
    let lines = run(&root, AgentToolId::ReadFile, "lines");
    assert_eq!(lines.status, ToolObservationStatus::Truncated);
    assert_eq!(lines.content.lines().count(), 400);
    let utf8 = run(&root, AgentToolId::ReadFile, "utf8");
    assert_eq!(utf8.status, ToolObservationStatus::Truncated);
    assert!(utf8.content.len() <= MAX_OUTPUT_BYTES);
    assert!(std::str::from_utf8(utf8.content.as_bytes()).is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn large_file_is_streamed_to_the_fixed_observation_cap() {
    let root = fixture("large-read");
    fs::write(root.join("large"), "가".repeat(1_000_000)).unwrap();
    let result = run(&root, AgentToolId::ReadFile, "large");
    assert_eq!(result.status, ToolObservationStatus::Truncated);
    assert!(result.content.len() <= MAX_OUTPUT_BYTES);
    assert!(result.truncation.original_bytes >= 3_000_000);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_directory_is_sorted_nonrecursive_and_reports_entry_kinds() {
    let root = fixture("list");
    fs::write(root.join("z"), "").unwrap();
    fs::write(root.join("a"), "").unwrap();
    fs::write(root.join(".hidden"), "").unwrap();
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/nested"), "").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("a", root.join("link")).unwrap();
    let result = run(&root, AgentToolId::ListDirectory, ".");
    #[cfg(unix)]
    assert_eq!(
        result.content,
        "file\ta\ndirectory\tdir\nsymlink\tlink\nfile\tz\n"
    );
    #[cfg(not(unix))]
    assert_eq!(result.content, "file\ta\ndirectory\tdir\nfile\tz\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn list_directory_caps_entry_count_and_output_bytes() {
    let root = fixture("list-caps");
    for index in 0..257 {
        fs::write(root.join(format!("entry-{index:03}")), "").unwrap();
    }
    let result = run(&root, AgentToolId::ListDirectory, ".");
    assert_eq!(result.status, ToolObservationStatus::Truncated);
    assert_eq!(result.content.lines().count(), 256);
    assert!(result.content.len() <= MAX_OUTPUT_BYTES);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn large_directory_stops_at_the_scan_cap() {
    let root = fixture("large-list");
    for index in 0..=1024 {
        fs::write(root.join(format!("entry-{index:04}")), "").unwrap();
    }
    let result = run(&root, AgentToolId::ListDirectory, ".");
    assert_eq!(result.status, ToolObservationStatus::Truncated);
    assert_eq!(result.content.lines().count(), 256);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_is_literal_and_excludes_hidden_ignored_and_symlinks() {
    let root = fixture("search");
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "a.b\naXb\n").unwrap();
    fs::write(root.join("ignored.txt"), "a.b\n").unwrap();
    fs::write(root.join(".hidden"), "a.b\n").unwrap();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("src/a.rs", root.join("linked.rs")).unwrap();
    let result = run(&root, AgentToolId::SearchRepository, "a.b");
    assert_eq!(result.content, "src/a.rs:1:a.b\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_caps_matches_and_each_visible_line() {
    let root = fixture("search-caps");
    for index in 0..65 {
        fs::write(root.join(format!("file-{index:03}")), "needle\n").unwrap();
    }
    let result = run(&root, AgentToolId::SearchRepository, "needle");
    assert_eq!(result.status, ToolObservationStatus::Truncated);
    assert_eq!(result.content.lines().count(), 64);
    fs::write(
        root.join("one-long-line"),
        format!("long{}\n", "x".repeat(600)),
    )
    .unwrap();
    let long = run(&root, AgentToolId::SearchRepository, "long");
    let visible = long
        .content
        .lines()
        .next()
        .unwrap()
        .splitn(3, ':')
        .nth(2)
        .unwrap();
    assert_eq!(visible.chars().count(), 512);
    assert!(result.content.len() <= MAX_OUTPUT_BYTES);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_streams_a_giant_single_line_without_losing_a_late_literal() {
    let root = fixture("search-giant-line");
    let mut contents = "x".repeat(2_000_000);
    contents.push_str("late-needle\n");
    fs::write(root.join("giant.txt"), contents).unwrap();
    let result = run(&root, AgentToolId::SearchRepository, "late-needle");
    assert_eq!(result.status, ToolObservationStatus::Ok);
    assert!(result.content.starts_with("giant.txt:1:"));
    assert!(result.content.len() < 600);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_search_stops_at_the_per_directory_scan_cap() {
    let root = fixture("search-large-dir");
    for index in 0..=2048 {
        fs::write(root.join(format!("file-{index:04}")), "unmatched\n").unwrap();
    }
    let result = run(&root, AgentToolId::SearchRepository, "needle");
    assert_eq!(result.status, ToolObservationStatus::Truncated);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fallback_search_reads_only_the_bounded_gitignore_prefix() {
    let root = fixture("search-large-ignore");
    fs::write(root.join("ignored.txt"), "needle\n").unwrap();
    fs::write(root.join("visible.txt"), "needle\n").unwrap();
    let mut ignore = String::from("ignored.txt\n");
    ignore.push_str(&format!("#{}\n", "x".repeat(100_000)));
    fs::write(root.join(".gitignore"), ignore).unwrap();
    let result = run(&root, AgentToolId::SearchRepository, "needle");
    assert_eq!(result.content, "visible.txt:1:needle\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_git_directory_overrides_external_core_worktree() {
    let root = fixture("git-local-worktree");
    let external = fixture("git-external-worktree");
    init_git(&root);
    fs::write(root.join("visible.txt"), "needle\n").unwrap();
    fs::write(external.join("outside.txt"), "needle\n").unwrap();
    let git = command::CommandPaths::resolve(&root)
        .path_for("git")
        .unwrap()
        .to_path_buf();
    let status = std::process::Command::new(git)
        .arg(format!("--git-dir={}", root.join(".git").display()))
        .args(["config", "core.worktree"])
        .arg(&external)
        .status()
        .unwrap();
    assert!(status.success());

    let search = run(&root, AgentToolId::SearchRepository, "needle");
    assert_eq!(search.content, "visible.txt:1:needle\n");
    let status = run(
        &root,
        AgentToolId::RunReadOnlyCommand,
        r#"["git","status","--short"]"#,
    );
    assert_eq!(status.status, ToolObservationStatus::Ok);
    assert!(status.content.contains("visible.txt"));
    assert!(!status.content.contains("outside.txt"));
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(external).unwrap();
}

#[test]
fn git_file_layout_is_denied_instead_of_falling_back_or_discovering_parent() {
    let root = fixture("git-file-layout");
    let external = fixture("git-file-target");
    init_git(&external);
    fs::write(
        root.join(".git"),
        format!("gitdir: {}\n", external.join(".git").display()),
    )
    .unwrap();
    fs::write(root.join("visible.txt"), "needle\n").unwrap();
    assert_eq!(
        run(&root, AgentToolId::SearchRepository, "needle").status,
        ToolObservationStatus::Denied
    );
    assert_eq!(
        run(
            &root,
            AgentToolId::RunReadOnlyCommand,
            r#"["git","status"]"#,
        )
        .status,
        ToolObservationStatus::Denied
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(external).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_git_directory_is_denied() {
    let root = fixture("git-symlink-layout");
    let external = fixture("git-symlink-target");
    init_git(&external);
    std::os::unix::fs::symlink(external.join(".git"), root.join(".git")).unwrap();
    assert_eq!(
        run(&root, AgentToolId::SearchRepository, "needle").status,
        ToolObservationStatus::Denied
    );
    assert_eq!(
        run(
            &root,
            AgentToolId::RunReadOnlyCommand,
            r#"["git","diff","--stat"]"#,
        )
        .status,
        ToolObservationStatus::Denied
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(external).unwrap();
}

#[test]
fn broken_local_repository_returns_tool_error_instead_of_fallback_search() {
    let root = fixture("git-broken-repository");
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join("visible.txt"), "needle\n").unwrap();
    let result = run(&root, AgentToolId::SearchRepository, "needle");
    assert_eq!(result.status, ToolObservationStatus::ToolError);
    assert!(!result.content.contains("visible.txt:1:needle"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_only_command_preserves_json_argv_and_checks_paths() {
    let root = fixture("commands");
    fs::write(root.join("space name.txt"), "literal phrase\n").unwrap();
    let head = run(
        &root,
        AgentToolId::RunReadOnlyCommand,
        r#"["head","-n","1","--","space name.txt"]"#,
    );
    assert_eq!(head.status, ToolObservationStatus::Ok);
    assert_eq!(head.content, "literal phrase\n");
    for denied_input in [
        r#"["head","-n","1","--","../outside"]"#,
        r#"["git","diff","--stat","--","../outside"]"#,
        r#"["git","diff","--stat","--","/absolute"]"#,
        r#"["git","status","--porcelain"]"#,
    ] {
        assert!(matches!(
            run(&root, AgentToolId::RunReadOnlyCommand, denied_input).status,
            ToolObservationStatus::Malformed | ToolObservationStatus::Denied
        ));
    }
    #[cfg(unix)]
    {
        fs::create_dir(root.join("real")).unwrap();
        std::os::unix::fs::symlink("real", root.join("linked-dir")).unwrap();
        let result = run(
            &root,
            AgentToolId::RunReadOnlyCommand,
            r#"["git","diff","--stat","--","linked-dir/missing"]"#,
        );
        assert_eq!(result.status, ToolObservationStatus::Denied);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellation_is_typed_before_execution() {
    let root = fixture("cancel");
    let cancellation = RequestCancellationToken::default();
    cancellation.cancel();
    let result = LocalToolExecutor {
        root: root.clone(),
        commands: command::CommandPaths::resolve(&root),
    }
    .execute(
        &call(AgentToolId::RunReadOnlyCommand, r#"["pwd"]"#),
        &cancellation,
        Duration::from_secs(1),
    );
    assert_eq!(result.status, ToolObservationStatus::Cancelled);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn zero_timeout_returns_typed_timeout() {
    let root = fixture("timeout");
    fs::write(root.join("file"), "contents").unwrap();
    let result = LocalToolExecutor {
        root: root.clone(),
        commands: command::CommandPaths::resolve(&root),
    }
    .execute(
        &call(AgentToolId::RunReadOnlyCommand, r#"["pwd"]"#),
        &RequestCancellationToken::default(),
        Duration::ZERO,
    );
    assert_eq!(result.status, ToolObservationStatus::Timeout);
    for (tool, input) in [
        (AgentToolId::ReadFile, "file"),
        (AgentToolId::ListDirectory, "."),
    ] {
        let result = LocalToolExecutor {
            root: root.clone(),
            commands: command::CommandPaths::resolve(&root),
        }
        .execute(
            &call(tool, input),
            &RequestCancellationToken::default(),
            Duration::ZERO,
        );
        assert_eq!(result.status, ToolObservationStatus::Timeout);
    }
    fs::remove_dir_all(root).unwrap();
}
