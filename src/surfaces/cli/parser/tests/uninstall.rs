use super::*;

#[test]
fn parses_uninstall_dry_run_purge_cache() {
    let command = parse([
        "uninstall".to_string(),
        "--dry-run".to_string(),
        "--purge-cache".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Uninstall(UninstallCommand::Plan {
            purge_cache: true,
            dry_run: true
        })
    );
}

#[test]
fn parses_guarded_clean_uninstall() {
    assert_eq!(
        parse([
            "uninstall".to_string(),
            "--clean".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap(),
        Command::Uninstall(UninstallCommand::CleanDryRun)
    );
    assert_eq!(
        parse([
            "uninstall".to_string(),
            "--clean".to_string(),
            "--yes".to_string(),
        ])
        .unwrap(),
        Command::Uninstall(UninstallCommand::CleanConfirmed)
    );
}

#[test]
fn clean_uninstall_requires_one_safety_mode_and_no_cache_mode() {
    let missing = parse(["uninstall".to_string(), "--clean".to_string()]).unwrap_err();
    assert_eq!(missing.code, 2);
    assert!(missing.message.contains("--dry-run"));
    assert!(missing.message.contains("--yes"));

    let conflicting = parse([
        "uninstall".to_string(),
        "--clean".to_string(),
        "--dry-run".to_string(),
        "--yes".to_string(),
    ])
    .unwrap_err();
    assert_eq!(conflicting.code, 2);
    assert!(conflicting.message.contains("동시에"));

    let mixed = parse([
        "uninstall".to_string(),
        "--clean".to_string(),
        "--keep-cache".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap_err();
    assert_eq!(mixed.code, 2);
    assert!(mixed.message.contains("함께"));
}
