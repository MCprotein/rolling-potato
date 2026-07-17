use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{BenchmarkCommand, BenchmarkReportFormat, ModelCommand};

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

pub(crate) fn run_benchmark(
    command: BenchmarkCommand,
    port: &mut impl BenchmarkCommandPort,
) -> Result<CommandOutput, AppError> {
    match command {
        BenchmarkCommand::Validate { path } => port.validate_report(&path).map(CommandOutput::Line),
        BenchmarkCommand::Record { fixture } => {
            port.record_report(&fixture).map(CommandOutput::Line)
        }
        BenchmarkCommand::Run {
            fixture,
            prompt,
            max_tokens,
        } => port
            .run_report(&fixture, &prompt, max_tokens)
            .map(CommandOutput::Line),
        BenchmarkCommand::Report { format } => port.report_export(format).map(CommandOutput::Exact),
    }
}

pub(crate) fn run_model(
    command: ModelCommand,
    port: &mut impl ModelCommandPort,
) -> Result<CommandOutput, AppError> {
    let report = match command {
        ModelCommand::List => port.list_report(),
        ModelCommand::Manifest => port.manifest_report(),
        ModelCommand::Inspect { id } => port.inspect_report(&id)?,
        ModelCommand::Registry => port.registry_report(),
        ModelCommand::Default => port.default_report()?,
        ModelCommand::SetDefault { id } => port.set_default_report(&id)?,
        ModelCommand::DownloadPlan { id } => port.download_plan_report(&id)?,
        ModelCommand::EvalPlan { id } => port.eval_plan_report(&id)?,
        ModelCommand::BenchmarkPlan { id } => port.benchmark_plan_report(&id)?,
        ModelCommand::FetchCandidate { id } => port.fetch_candidate_report(&id)?,
        ModelCommand::VerifyFile { path, sha256 } => port.verify_file_report(&path, &sha256)?,
        ModelCommand::Promote { id, evidence } => port.promote_candidate_report(&id, &evidence)?,
        ModelCommand::CleanupFailed { id, dry_run } => port.cleanup_failed_report(&id, dry_run)?,
        ModelCommand::Install { id } => {
            port.install_candidate(&id)?;
            return Ok(CommandOutput::None);
        }
    };
    Ok(CommandOutput::Line(report))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        fn promote_candidate_report(
            &mut self,
            id: &str,
            evidence: &str,
        ) -> Result<String, AppError> {
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
}
