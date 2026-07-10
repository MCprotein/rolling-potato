mod app;
mod approval;
mod backend;
mod benchmark;
mod cache;
mod checksum;
mod cli;
mod config;
mod context;
mod evidence;
mod hooks;
mod intent;
mod korean_guard;
mod lease;
mod ledger;
mod model;
mod monitor;
mod observability;
mod ontology;
mod patch;
mod paths;
mod plugin;
mod policy;
mod resource;
mod runtime;
mod skill;
mod state;
mod strict_json;
mod team;
#[cfg(test)]
mod test_support;
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
