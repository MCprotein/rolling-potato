mod adapters;
mod app;
mod approval;
mod backend;
mod backend_stream;
mod benchmark;
mod cli;
mod composition;
mod context;
mod evidence;
mod foundation;
mod hooks;
mod intent;
mod korean_guard;
mod ledger;
mod model;
mod monitor;
mod observability;
mod ontology;
mod patch;
mod plugin;
mod policy;
mod resource;
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
mod test_support;
mod transcript;
mod transition;
mod tui;
mod uninstall;

use std::process::ExitCode;

fn main() -> ExitCode {
    match app::run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", korean_guard::guard_or_failure(&err.message));
            ExitCode::from(err.code)
        }
    }
}
