mod app;
mod backend;
mod cache;
mod cli;
mod config;
mod evidence;
mod hooks;
mod intent;
mod ledger;
mod model;
mod monitor;
mod observability;
mod paths;
mod plugin;
mod policy;
mod runtime;
mod skill;
mod state;
#[cfg(test)]
mod test_support;
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
