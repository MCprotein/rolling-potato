use super::*;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
}

fn assert_file(path: &str) {
    assert!(
        Path::new(path).is_file(),
        "missing collaboration owner: {path}"
    );
}

fn assert_registered(source: &str, module: &str, context: &str) {
    assert!(
        source.lines().any(|line| line == module),
        "{context} does not register owner: {module}"
    );
}

fn assert_moved(owner: &str, facade: &str, responsibility: &str) {
    assert!(
        owner.contains(responsibility),
        "collaboration owner is missing responsibility: {responsibility}"
    );
    assert!(
        !facade.contains(responsibility),
        "collaboration facade still owns responsibility: {responsibility}"
    );
}

fn assert_line_bound(source: &str, maximum_lines: usize, owner: &str) {
    assert!(
        source.lines().count() < maximum_lines,
        "collaboration module regrew beyond its ownership boundary: {owner}"
    );
}

#[path = "collaboration/domain.rs"]
mod domain;
#[path = "collaboration/subagent.rs"]
mod subagent;
#[path = "collaboration/team.rs"]
mod team;
#[path = "collaboration/team_reconciliation.rs"]
mod team_reconciliation;
