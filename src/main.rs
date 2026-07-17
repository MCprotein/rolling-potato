mod adapters;
mod app;
mod approval;
mod composition;
mod context;
mod evidence;
mod foundation;
mod hooks;
mod intent;
mod ledger;
mod monitor;
mod observability;
mod ontology;
mod patch;
mod plugin;
mod policy;
mod runtime;
mod runtime_core;
mod skill;
mod state;
mod subagent;
mod subagent_result;
mod surfaces;
mod team;
mod team_execution;
mod team_reconciliation;
pub mod team_state;
#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;
mod transcript;
mod transition;
mod tui;

fn main() -> std::process::ExitCode {
    composition::startup::run(std::env::args().skip(1), app::run)
}
