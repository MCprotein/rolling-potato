use std::io::Write;

use crate::foundation::error::AppError;
use crate::surfaces::cli::command::BenchmarkReportFormat;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CommandOutput {
    Line(String),
    Exact(String),
    None,
}

pub(crate) trait BenchmarkCommandPort {
    fn validate_report(&mut self, path: &str) -> Result<String, AppError>;
    fn record_report(&mut self, fixture: &str) -> Result<String, AppError>;
    fn run_report(
        &mut self,
        fixture: &str,
        prompt: &str,
        max_tokens: Option<u32>,
    ) -> Result<String, AppError>;
    fn report_export(&mut self, format: BenchmarkReportFormat) -> Result<String, AppError>;
}

pub(crate) trait BackendCommandPort {
    fn doctor_report(&mut self) -> String;
    fn install_plan_report(&mut self) -> String;
    fn install_report(&mut self) -> Result<String, AppError>;
    fn default_model_path(&mut self) -> Result<String, AppError>;
    fn start_report(&mut self, model_path: &str, ctx_size: Option<u32>)
        -> Result<String, AppError>;
    fn status_report(&mut self) -> Result<String, AppError>;
    fn stop_report(&mut self) -> Result<String, AppError>;
    fn cancel_generation_report(&mut self) -> Result<String, AppError>;
    fn verify_archive_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError>;
    fn health_check_report(&mut self) -> String;
    fn chat_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<String, AppError>;
    fn chat_stream_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
        writer: &mut impl Write,
    ) -> Result<String, AppError>;
}

pub(crate) trait ModelCommandPort {
    fn list_report(&mut self) -> String;
    fn manifest_report(&mut self) -> String;
    fn inspect_report(&mut self, id: &str) -> Result<String, AppError>;
    fn registry_report(&mut self) -> String;
    fn default_report(&mut self) -> Result<String, AppError>;
    fn set_default_report(&mut self, id: &str) -> Result<String, AppError>;
    fn download_plan_report(&mut self, id: &str) -> Result<String, AppError>;
    fn eval_plan_report(&mut self, id: &str) -> Result<String, AppError>;
    fn benchmark_plan_report(&mut self, id: &str) -> Result<String, AppError>;
    fn fetch_candidate_report(&mut self, id: &str) -> Result<String, AppError>;
    fn verify_file_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError>;
    fn promote_candidate_report(&mut self, id: &str, evidence: &str) -> Result<String, AppError>;
    fn cleanup_failed_report(&mut self, id: &str, dry_run: bool) -> Result<String, AppError>;
    fn install_candidate(&mut self, id: &str) -> Result<(), AppError>;
}
