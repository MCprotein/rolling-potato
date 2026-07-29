use std::io::Write;

use crate::foundation::error::AppError;
use crate::surfaces::cli::command::BackendCommand;

use super::{BackendCommandPort, CommandOutput};

pub(crate) fn run_backend(
    command: BackendCommand,
    port: &mut impl BackendCommandPort,
    writer: &mut impl Write,
) -> Result<CommandOutput, AppError> {
    let report = match command {
        BackendCommand::Doctor => port.doctor_report(),
        BackendCommand::InstallPlan => port.install_plan_report(),
        BackendCommand::Install => port.install_report()?,
        BackendCommand::Start {
            model_path,
            ctx_size,
        } => {
            let model_path = match model_path {
                Some(path) => path,
                None => port.default_model_path()?,
            };
            port.start_report(&model_path, ctx_size)?
        }
        BackendCommand::Status => port.status_report()?,
        BackendCommand::Stop => port.stop_report()?,
        BackendCommand::Cancel => port.cancel_generation_report()?,
        BackendCommand::VerifyArchive { path, sha256 } => {
            port.verify_archive_report(&path, &sha256)?
        }
        BackendCommand::HealthCheck => port.health_check_report(),
        BackendCommand::Chat {
            prompt,
            max_tokens,
            stream,
            timeout_ms,
        } => {
            if stream {
                port.chat_stream_report(&prompt, max_tokens, timeout_ms, writer)?
            } else {
                port.chat_report(&prompt, max_tokens, timeout_ms)?
            }
        }
    };
    Ok(CommandOutput::Line(report))
}
