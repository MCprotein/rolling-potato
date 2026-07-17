mod adapters;
mod app;
mod composition;
mod context;
mod evidence;
mod foundation;
mod intent;
mod ledger;
mod ontology;
mod patch;
mod runtime;
mod runtime_core;
mod state;
mod surfaces;
mod team;
mod team_reconciliation;
#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;
mod transcript;
mod transition;
mod tui;

fn main() -> std::process::ExitCode {
    composition::startup::run(std::env::args().skip(1), app::run)
}
