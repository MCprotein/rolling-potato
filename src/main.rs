mod adapters;
mod app;
mod composition;
mod foundation;
mod runtime_core;
mod surfaces;
#[cfg(test)]
#[path = "../tests/support/runtime_fixture.rs"]
mod test_support;

fn main() -> std::process::ExitCode {
    composition::startup::run(std::env::args().skip(1), app::run)
}
