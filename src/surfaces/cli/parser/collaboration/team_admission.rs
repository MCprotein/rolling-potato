use super::shared::parse_write_owner_for;
use super::{AppError, TeamCommand};

pub(in super::super) fn parse_team_admit_args(args: &[String]) -> Result<TeamCommand, AppError> {
    let mut lanes = None;
    let mut write_paths = Vec::new();
    let mut owned_write_paths = Vec::new();
    let mut commands = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--lanes" => {
                if lanes.is_some() {
                    return Err(AppError::usage(
                        "team admit의 --lanes 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(AppError::usage(
                        "team admit은 --lanes <count> 값이 필요합니다.",
                    ));
                };
                let parsed = value.parse::<u32>().map_err(|_| {
                    AppError::usage("team admit의 --lanes 값은 양의 정수여야 합니다.")
                })?;
                if parsed == 0 {
                    return Err(AppError::usage(
                        "team admit의 --lanes 값은 1 이상이어야 합니다.",
                    ));
                }
                lanes = Some(parsed);
                index += 1;
            }
            "--write" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(AppError::usage(
                        "team admit은 --write <path> 값이 필요합니다.",
                    ));
                };
                if value.starts_with("--") {
                    return Err(AppError::usage(
                        "team admit은 --write <path> 값이 필요합니다.",
                    ));
                }
                write_paths.push(value.clone());
                index += 1;
            }
            "--write-owner" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(AppError::usage(
                        "team admit은 --write-owner <lane:path> 값이 필요합니다.",
                    ));
                };
                if value.starts_with("--") {
                    return Err(AppError::usage(
                        "team admit은 --write-owner <lane:path> 값이 필요합니다.",
                    ));
                }
                let (lane, path) = parse_write_owner_for(value, "team admit")?;
                owned_write_paths.push((lane, path));
                index += 1;
            }
            "--command" => {
                index += 1;
                let start = index;
                while index < args.len() && !args[index].starts_with("--") {
                    index += 1;
                }
                if start == index {
                    return Err(AppError::usage(
                        "team admit은 --command <command> 값이 필요합니다.",
                    ));
                }
                commands.push(args[start..index].join(" "));
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 team admit 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let lanes =
        lanes.ok_or_else(|| AppError::usage("team admit은 --lanes <count> 형식이 필요합니다."))?;
    if let Some((lane, _)) = owned_write_paths.iter().find(|(lane, _)| *lane > lanes) {
        return Err(AppError::usage(format!(
            "team admit의 --write-owner lane {lane}은 --lanes {lanes} 값을 넘을 수 없습니다."
        )));
    }

    Ok(TeamCommand::Admit {
        lanes,
        write_paths,
        owned_write_paths,
        commands,
    })
}
