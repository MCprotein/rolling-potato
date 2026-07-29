use crate::foundation::error::AppError;
use crate::surfaces::cli::command::BenchmarkCommand;

use super::{BenchmarkCommandPort, CommandOutput};

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
