use crate::foundation::error::AppError;
use crate::surfaces::cli::command::ModelCommand;

pub(super) fn parse_model(args: &[String]) -> Result<ModelCommand, AppError> {
    match args {
        [action] if action == "list" => Ok(ModelCommand::List),
        [action] if action == "manifest" => Ok(ModelCommand::Manifest),
        [action, id] if action == "inspect" => Ok(ModelCommand::Inspect { id: id.clone() }),
        [action] if action == "registry" => Ok(ModelCommand::Registry),
        [action] if action == "default" => Ok(ModelCommand::Default),
        [action, id] if action == "default" => {
            Ok(ModelCommand::SetDefault { id: id.clone() })
        }
        [action, id] if action == "download-plan" => {
            Ok(ModelCommand::DownloadPlan { id: id.clone() })
        }
        [action, id] if action == "eval-plan" => {
            Ok(ModelCommand::EvalPlan { id: id.clone() })
        }
        [action, id] if action == "benchmark-plan" => {
            Ok(ModelCommand::BenchmarkPlan { id: id.clone() })
        }
        [action, id, flag] if action == "fetch-candidate" && flag == "--for-evaluation" => {
            Ok(ModelCommand::FetchCandidate { id: id.clone() })
        }
        [action, ..] if action == "fetch-candidate" => Err(AppError::usage(
            "model fetch-candidate는 <id> --for-evaluation 형식이 필요합니다.",
        )),
        [action, path, flag, sha256] if action == "verify-file" && flag == "--sha256" => {
            Ok(ModelCommand::VerifyFile {
                path: path.clone(),
                sha256: sha256.clone(),
            })
        }
        [action, ..] if action == "verify-file" => Err(AppError::usage(
            "model verify-file은 <path> --sha256 <hash> 형식이 필요합니다.",
        )),
        [action, id, flag, evidence] if action == "promote" && flag == "--evidence" => {
            Ok(ModelCommand::Promote {
                id: id.clone(),
                evidence: evidence.clone(),
            })
        }
        [action, ..] if action == "promote" => Err(AppError::usage(
            "model promote는 <id> --evidence <file> 형식이 필요합니다.",
        )),
        [action, id, flag] if action == "cleanup-failed" => {
            let dry_run = match flag.as_str() {
                "--dry-run" => true,
                "--delete" => false,
                _ => {
                    return Err(AppError::usage(
                        "model cleanup-failed는 --dry-run 또는 --delete가 필요합니다.",
                    ));
                }
            };
            Ok(ModelCommand::CleanupFailed {
                id: id.clone(),
                dry_run,
            })
        }
        [action, ..] if action == "cleanup-failed" => Err(AppError::usage(
            "model cleanup-failed는 <id> --dry-run 또는 <id> --delete 형식이 필요합니다.",
        )),
        [action, id] if action == "install" => Ok(ModelCommand::Install { id: id.clone() }),
        [action] if action == "install" => Err(AppError::usage(
            "모델 id가 필요합니다. 예: rpotato model install qwen3.5-4b",
        )),
        _ => Err(AppError::usage(
            "model 명령은 list, manifest, inspect, registry, default, download-plan, eval-plan, benchmark-plan, fetch-candidate, verify-file, promote, cleanup-failed, install만 허용합니다.",
        )),
    }
}
