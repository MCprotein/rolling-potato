mod app;
mod approval;
mod backend;
mod cache;
mod checksum;
mod cli;
mod config;
mod context;
mod evidence;
mod hooks;
mod intent;
mod ledger;
mod model;
mod monitor;
mod observability;
mod patch;
mod paths;
mod plugin;
mod policy;
mod resource;
mod runtime;
mod skill;
mod state;
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
            eprintln!("{}", err.message);
            ExitCode::from(err.code)
        }
    }
}
