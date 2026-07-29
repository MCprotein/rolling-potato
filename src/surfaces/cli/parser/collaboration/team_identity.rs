use super::{AppError, TeamCommand};

pub(in super::super) fn parse_team_plan_args(args: &[String]) -> Result<TeamCommand, AppError> {
    let mut manifest_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err(AppError::usage(
                        "team plan의 --manifest 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team plan은 --manifest <project-relative-json> 값이 필요합니다.",
                    ));
                };
                if value.starts_with("--") || value.trim().is_empty() {
                    return Err(AppError::usage(
                        "team plan은 --manifest <project-relative-json> 값이 필요합니다.",
                    ));
                }
                manifest_path = Some(value.clone());
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 team plan 옵션입니다: {unknown}"
                )));
            }
        }
    }
    Ok(TeamCommand::Plan {
        manifest_path: manifest_path.ok_or_else(|| {
            AppError::usage("team plan은 --manifest <project-relative-json> 형식이 필요합니다.")
        })?,
    })
}

pub(in super::super) fn parse_team_execute_args(args: &[String]) -> Result<TeamCommand, AppError> {
    Ok(TeamCommand::Execute {
        team_id: parse_team_id_args(args, "team execute")?,
    })
}

pub(in super::super) fn parse_team_reconcile_args(
    args: &[String],
) -> Result<TeamCommand, AppError> {
    Ok(TeamCommand::Reconcile {
        team_id: parse_team_id_args(args, "team reconcile")?,
    })
}

pub(in super::super) fn parse_team_cancel_args(args: &[String]) -> Result<TeamCommand, AppError> {
    Ok(TeamCommand::Cancel {
        team_id: parse_team_id_args(args, "team cancel")?,
    })
}

fn parse_team_id_args(args: &[String], command: &str) -> Result<String, AppError> {
    let mut team_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--team" => {
                if team_id.is_some() {
                    return Err(AppError::usage(format!(
                        "{command}: --team 옵션은 한 번만 지정할 수 있습니다."
                    )));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(format!(
                        "{command}: --team <team-id> 값이 필요합니다."
                    )));
                };
                if value.starts_with("--") || value.trim().is_empty() {
                    return Err(AppError::usage(format!(
                        "{command}: --team <team-id> 값이 필요합니다."
                    )));
                }
                team_id = Some(value.clone());
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 {command} 옵션입니다: {unknown}"
                )));
            }
        }
    }
    team_id
        .ok_or_else(|| AppError::usage(format!("{command}: --team <team-id> 형식이 필요합니다.")))
}
