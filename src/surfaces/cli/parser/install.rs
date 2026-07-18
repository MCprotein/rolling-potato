use crate::foundation::error::AppError;
use crate::surfaces::cli::command::InstallCommand;

pub(super) fn parse_install(args: &[String]) -> Result<InstallCommand, AppError> {
    let mut clean = false;
    let mut dry_run = false;
    let mut confirmed = false;

    for arg in args {
        match arg.as_str() {
            "--clean" => clean = true,
            "--dry-run" => dry_run = true,
            "--yes" => confirmed = true,
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 install 옵션입니다: {unknown}"
                )));
            }
        }
    }

    match (clean, dry_run, confirmed) {
        (false, false, false) => Ok(InstallCommand::Standard),
        (true, true, false) => Ok(InstallCommand::CleanDryRun),
        (true, false, true) => Ok(InstallCommand::CleanConfirmed),
        (true, false, false) => Err(AppError::usage(
            "clean install은 삭제 확인이 필요합니다. 먼저 `rpotato install --clean --dry-run`으로 확인한 뒤 `rpotato install --clean --yes`를 실행하세요.",
        )),
        (false, _, _) => Err(AppError::usage(
            "--dry-run과 --yes는 --clean과 함께만 사용할 수 있습니다.",
        )),
        (true, true, true) => Err(AppError::usage(
            "--dry-run과 --yes는 동시에 사용할 수 없습니다.",
        )),
    }
}
