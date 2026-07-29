//! Inference command composition facade.

#[path = "inference/backend.rs"]
mod backend;
#[path = "inference/benchmark.rs"]
mod benchmark;
#[path = "inference/model.rs"]
mod model;
#[path = "inference/ports.rs"]
mod ports;

pub(crate) use backend::run_backend;
pub(crate) use benchmark::run_benchmark;
pub(crate) use model::run_model;
pub(crate) use ports::{BackendCommandPort, BenchmarkCommandPort, CommandOutput, ModelCommandPort};

#[cfg(test)]
#[path = "inference/tests.rs"]
mod tests;
