use crate::foundation::error::AppError;
use crate::surfaces::cli::command::ModelCommand;

use super::{CommandOutput, ModelCommandPort};

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
