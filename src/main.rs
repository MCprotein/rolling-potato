mod adapters;
mod app;
mod composition;
mod evidence;
mod foundation;
mod intent;
mod ontology;
mod patch;
mod runtime_core;
mod surfaces;
#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;

fn main() -> std::process::ExitCode {
    composition::startup::run(std::env::args().skip(1), app::run)
}
