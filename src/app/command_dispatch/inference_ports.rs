//! Concrete inference command ports used by the CLI dispatch adapter.

use super::{inference, AppError, CommandDispatchAdapter};
use crate::app::inference_adapter::{backend, benchmark, model};

impl inference::BenchmarkCommandPort for CommandDispatchAdapter {
    fn validate_report(&mut self, path: &str) -> Result<String, AppError> {
        benchmark::validate_report(path)
    }

    fn record_report(&mut self, fixture: &str) -> Result<String, AppError> {
        benchmark::record_report(fixture)
    }

    fn run_report(
        &mut self,
        fixture: &str,
        prompt: &str,
        max_tokens: Option<u32>,
    ) -> Result<String, AppError> {
        benchmark::run_report(fixture, prompt, max_tokens)
    }

    fn report_export(
        &mut self,
        format: crate::surfaces::cli::command::BenchmarkReportFormat,
    ) -> Result<String, AppError> {
        benchmark::report_export(format)
    }
}

impl inference::BackendCommandPort for CommandDispatchAdapter {
    fn doctor_report(&mut self) -> String {
        backend::doctor_report()
    }

    fn install_plan_report(&mut self) -> String {
        backend::install_plan_report()
    }

    fn install_report(&mut self) -> Result<String, AppError> {
        backend::install_report()
    }

    fn default_model_path(&mut self) -> Result<String, AppError> {
        Ok(model::default_artifact_path()?.display().to_string())
    }

    fn start_report(
        &mut self,
        model_path: &str,
        ctx_size: Option<u32>,
    ) -> Result<String, AppError> {
        backend::start_report(model_path, ctx_size)
    }

    fn status_report(&mut self) -> Result<String, AppError> {
        backend::status_report()
    }

    fn stop_report(&mut self) -> Result<String, AppError> {
        backend::stop_report()
    }

    fn cancel_generation_report(&mut self) -> Result<String, AppError> {
        backend::cancel_generation_report()
    }

    fn verify_archive_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError> {
        backend::verify_archive_report(path, sha256)
    }

    fn health_check_report(&mut self) -> String {
        backend::health_check_report()
    }

    fn chat_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<String, AppError> {
        backend::chat_report(prompt, max_tokens, timeout_ms)
    }

    fn chat_stream_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
        writer: &mut impl std::io::Write,
    ) -> Result<String, AppError> {
        backend::chat_stream_report(prompt, max_tokens, timeout_ms, writer)
    }
}

impl inference::ModelCommandPort for CommandDispatchAdapter {
    fn list_report(&mut self) -> String {
        model::list_report()
    }

    fn manifest_report(&mut self) -> String {
        model::manifest_report()
    }

    fn inspect_report(&mut self, id: &str) -> Result<String, AppError> {
        model::inspect_report(id)
    }

    fn registry_report(&mut self) -> String {
        model::registry_report()
    }

    fn default_report(&mut self) -> Result<String, AppError> {
        model::default_report()
    }

    fn set_default_report(&mut self, id: &str) -> Result<String, AppError> {
        model::set_default_report(id)
    }

    fn download_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::download_plan_report(id)
    }

    fn eval_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::eval_plan_report(id)
    }

    fn benchmark_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::benchmark_plan_report(id)
    }

    fn fetch_candidate_report(&mut self, id: &str) -> Result<String, AppError> {
        model::fetch_candidate_for_evaluation_report(id)
    }

    fn verify_file_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError> {
        model::verify_file_report(path, sha256)
    }

    fn promote_candidate_report(&mut self, id: &str, evidence: &str) -> Result<String, AppError> {
        model::promote_candidate_report(id, evidence)
    }

    fn cleanup_failed_report(&mut self, id: &str, dry_run: bool) -> Result<String, AppError> {
        model::cleanup_failed_report(id, dry_run)
    }

    fn install_candidate(&mut self, id: &str) -> Result<(), AppError> {
        model::install_candidate(id)
    }
}

pub(super) fn emit_output(output: inference::CommandOutput) {
    match output {
        inference::CommandOutput::Line(report) => println!("{report}"),
        inference::CommandOutput::Exact(report) => print!("{report}"),
        inference::CommandOutput::None => {}
    }
}
