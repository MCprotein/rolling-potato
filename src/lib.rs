//! Library composition facade for the `rpotato` binary.

mod adapters;
mod app;
mod composition;
mod foundation;
mod runtime_core;
mod surfaces;

#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;

/// Runs the CLI through the canonical startup and application composition boundary.
pub fn run(args: impl IntoIterator<Item = String>) -> std::process::ExitCode {
    composition::startup::run(args, app::run)
}
