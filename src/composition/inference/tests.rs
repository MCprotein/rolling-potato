use super::*;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{
    BackendCommand, BenchmarkCommand, BenchmarkReportFormat, ModelCommand,
};
use std::io::Write;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Validate(String),
    Record(String),
    Run(String, String, Option<u32>),
    Report(BenchmarkReportFormat),
}

#[derive(Default)]
struct RecordingPort {
    calls: Vec<Call>,
}

impl BenchmarkCommandPort for RecordingPort {
    fn validate_report(&mut self, path: &str) -> Result<String, AppError> {
        self.calls.push(Call::Validate(path.to_owned()));
        Ok("validated".to_owned())
    }

    fn record_report(&mut self, fixture: &str) -> Result<String, AppError> {
        self.calls.push(Call::Record(fixture.to_owned()));
        Ok("recorded".to_owned())
    }

    fn run_report(
        &mut self,
        fixture: &str,
        prompt: &str,
        max_tokens: Option<u32>,
    ) -> Result<String, AppError> {
        self.calls
            .push(Call::Run(fixture.to_owned(), prompt.to_owned(), max_tokens));
        Ok("ran".to_owned())
    }

    fn report_export(&mut self, format: BenchmarkReportFormat) -> Result<String, AppError> {
        self.calls.push(Call::Report(format));
        Ok("export".to_owned())
    }
}

#[derive(Default)]
struct RecordingBackendPort {
    calls: Vec<String>,
}

impl BackendCommandPort for RecordingBackendPort {
    fn doctor_report(&mut self) -> String {
        unreachable!()
    }

    fn install_plan_report(&mut self) -> String {
        unreachable!()
    }

    fn install_report(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn default_model_path(&mut self) -> Result<String, AppError> {
        self.calls.push("default-model".to_owned());
        Ok("default.gguf".to_owned())
    }

    fn start_report(
        &mut self,
        model_path: &str,
        ctx_size: Option<u32>,
    ) -> Result<String, AppError> {
        self.calls.push(format!("start:{model_path}:{ctx_size:?}"));
        Ok("started".to_owned())
    }

    fn status_report(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn stop_report(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn cancel_generation_report(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn verify_archive_report(&mut self, _path: &str, _sha256: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn health_check_report(&mut self) -> String {
        unreachable!()
    }

    fn chat_report(
        &mut self,
        _prompt: &str,
        _max_tokens: Option<u32>,
        _timeout_ms: Option<u32>,
    ) -> Result<String, AppError> {
        unreachable!()
    }

    fn chat_stream_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
        writer: &mut impl Write,
    ) -> Result<String, AppError> {
        self.calls
            .push(format!("stream:{prompt}:{max_tokens:?}:{timeout_ms:?}"));
        writer.write_all(b"delta").unwrap();
        Ok("streamed".to_owned())
    }
}

#[derive(Default)]
struct RecordingModelPort {
    calls: Vec<String>,
}

impl ModelCommandPort for RecordingModelPort {
    fn list_report(&mut self) -> String {
        unreachable!()
    }

    fn manifest_report(&mut self) -> String {
        unreachable!()
    }

    fn inspect_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn registry_report(&mut self) -> String {
        unreachable!()
    }

    fn default_report(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn set_default_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn download_plan_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn eval_plan_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn benchmark_plan_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn fetch_candidate_report(&mut self, _id: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn verify_file_report(&mut self, _path: &str, _sha256: &str) -> Result<String, AppError> {
        unreachable!()
    }

    fn promote_candidate_report(&mut self, id: &str, evidence: &str) -> Result<String, AppError> {
        self.calls.push(format!("promote:{id}:{evidence}"));
        Ok("promoted".to_owned())
    }

    fn cleanup_failed_report(&mut self, _id: &str, _dry_run: bool) -> Result<String, AppError> {
        unreachable!()
    }

    fn install_candidate(&mut self, id: &str) -> Result<(), AppError> {
        self.calls.push(format!("install:{id}"));
        Ok(())
    }
}

#[test]
fn run_preserves_arguments_and_line_output() {
    let mut port = RecordingPort::default();

    let output = run_benchmark(
        BenchmarkCommand::Run {
            fixture: "fixture.json".to_owned(),
            prompt: "prompt.txt".to_owned(),
            max_tokens: Some(32),
        },
        &mut port,
    )
    .unwrap();

    assert_eq!(output, CommandOutput::Line("ran".to_owned()));
    assert_eq!(
        port.calls,
        [Call::Run(
            "fixture.json".to_owned(),
            "prompt.txt".to_owned(),
            Some(32)
        )]
    );
}

#[test]
fn report_uses_exact_output_without_added_newline() {
    let mut port = RecordingPort::default();

    let output = run_benchmark(
        BenchmarkCommand::Report {
            format: BenchmarkReportFormat::Jsonl,
        },
        &mut port,
    )
    .unwrap();

    assert_eq!(output, CommandOutput::Exact("export".to_owned()));
    assert_eq!(port.calls, [Call::Report(BenchmarkReportFormat::Jsonl)]);
}

#[test]
fn backend_start_resolves_default_model_before_start() {
    let mut port = RecordingBackendPort::default();
    let mut writer = Vec::new();

    let output = run_backend(
        BackendCommand::Start {
            model_path: None,
            ctx_size: Some(4096),
        },
        &mut port,
        &mut writer,
    )
    .unwrap();

    assert_eq!(output, CommandOutput::Line("started".to_owned()));
    assert_eq!(
        port.calls,
        ["default-model", "start:default.gguf:Some(4096)"]
    );
    assert!(writer.is_empty());
}

#[test]
fn backend_stream_writes_deltas_before_returning_summary() {
    let mut port = RecordingBackendPort::default();
    let mut writer = Vec::new();

    let output = run_backend(
        BackendCommand::Chat {
            prompt: "hello".to_owned(),
            max_tokens: Some(16),
            stream: true,
            timeout_ms: Some(500),
        },
        &mut port,
        &mut writer,
    )
    .unwrap();

    assert_eq!(writer, b"delta");
    assert_eq!(output, CommandOutput::Line("streamed".to_owned()));
    assert_eq!(port.calls, ["stream:hello:Some(16):Some(500)"]);
}

#[test]
fn model_command_preserves_arguments_and_line_output() {
    let mut port = RecordingModelPort::default();

    let output = run_model(
        ModelCommand::Promote {
            id: "model-a".to_owned(),
            evidence: "evidence.json".to_owned(),
        },
        &mut port,
    )
    .unwrap();

    assert_eq!(output, CommandOutput::Line("promoted".to_owned()));
    assert_eq!(port.calls, ["promote:model-a:evidence.json"]);
}

#[test]
fn model_install_has_no_command_output() {
    let mut port = RecordingModelPort::default();

    let output = run_model(
        ModelCommand::Install {
            id: "model-a".to_owned(),
        },
        &mut port,
    )
    .unwrap();

    assert_eq!(output, CommandOutput::None);
    assert_eq!(port.calls, ["install:model-a"]);
}
