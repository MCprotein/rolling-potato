mod adapters;
mod app;
mod composition;
mod context;
mod evidence;
mod foundation;
mod intent;
mod ontology;
mod patch;
mod runtime;
mod runtime_core;
mod state;
mod surfaces;
#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;
mod transition;
mod tui;

fn main() -> std::process::ExitCode {
    composition::startup::run(std::env::args().skip(1), app::run)
}
