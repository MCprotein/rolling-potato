use super::*;

#[test]
fn parses_standard_and_guarded_clean_install() {
    assert_eq!(
        parse(["install".to_string()]).unwrap(),
        Command::Install(InstallCommand::Standard)
    );
    assert_eq!(
        parse([
            "install".to_string(),
            "--clean".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap(),
        Command::Install(InstallCommand::CleanDryRun)
    );
    assert_eq!(
        parse([
            "install".to_string(),
            "--clean".to_string(),
            "--yes".to_string(),
        ])
        .unwrap(),
        Command::Install(InstallCommand::CleanConfirmed)
    );
}

#[test]
fn clean_install_requires_exactly_one_safety_mode() {
    let missing_confirmation = parse(["install".to_string(), "--clean".to_string()]).unwrap_err();
    assert_eq!(missing_confirmation.code, 2);
    assert!(missing_confirmation.message.contains("--dry-run"));
    assert!(missing_confirmation.message.contains("--yes"));

    let conflicting = parse([
        "install".to_string(),
        "--clean".to_string(),
        "--dry-run".to_string(),
        "--yes".to_string(),
    ])
    .unwrap_err();
    assert_eq!(conflicting.code, 2);
    assert!(conflicting.message.contains("동시에"));
}
