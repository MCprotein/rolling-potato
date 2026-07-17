use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{BenchmarkCommand, BenchmarkReportFormat};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CommandOutput {
    Line(String),
    Exact(String),
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
}
