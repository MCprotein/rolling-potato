use super::*;

#[test]
fn parses_update_apply_and_check() {
    assert_eq!(
        parse(["update".to_string()]).unwrap(),
        Command::Update(UpdateCommand::Apply)
    );
    assert_eq!(
        parse(["update".to_string(), "--check".to_string()]).unwrap(),
        Command::Update(UpdateCommand::Check)
    );
}

#[test]
fn rejects_unknown_update_options() {
    let error = parse(["update".to_string(), "--force".to_string()]).unwrap_err();
    assert_eq!(error.code, 2);
    assert!(error.message.contains("--check"));
}
