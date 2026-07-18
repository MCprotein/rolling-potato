use crate::foundation::error::AppError;
use crate::surfaces::cli::command::UninstallCommand;

pub(super) fn parse_uninstall(args: &[String]) -> Result<UninstallCommand, AppError> {
    let mut keep_cache = false;
    let mut purge_cache = false;
    let mut clean = false;
    let mut dry_run = false;
    let mut confirmed = false;

    for arg in args {
        match arg.as_str() {
            "--keep-cache" => keep_cache = true,
            "--purge-cache" => purge_cache = true,
            "--clean" => clean = true,
            "--dry-run" => dry_run = true,
            "--yes" => confirmed = true,
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 uninstall 옵션입니다: {unknown}"
                )));
            }
        }
    }

    if clean {
        if keep_cache || purge_cache {
            return Err(AppError::usage(
                "--clean은 --keep-cache 또는 --purge-cache와 함께 사용할 수 없습니다.",
            ));
        }
        return match (dry_run, confirmed) {
            (true, false) => Ok(UninstallCommand::CleanDryRun),
            (false, true) => Ok(UninstallCommand::CleanConfirmed),
            (false, false) => Err(AppError::usage(
                "clean uninstall은 삭제 확인이 필요합니다. 먼저 `rpotato uninstall --clean --dry-run`으로 확인한 뒤 `rpotato uninstall --clean --yes`를 실행하세요.",
            )),
            (true, true) => Err(AppError::usage(
                "--dry-run과 --yes는 동시에 사용할 수 없습니다.",
            )),
        };
    }

    if confirmed {
        return Err(AppError::usage(
            "--yes는 --clean과 함께만 사용할 수 있습니다.",
        ));
    }
    if keep_cache == purge_cache {
        return Err(AppError::usage(
            "uninstall은 --keep-cache 또는 --purge-cache 중 하나가 필요합니다.",
        ));
    }

    Ok(UninstallCommand::Plan {
        purge_cache,
        dry_run,
    })
}
