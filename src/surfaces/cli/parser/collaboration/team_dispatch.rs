use super::shared::parse_write_owner_for;
use super::{parse_positive_u32, AppError, ModelTier, TeamCommand};

pub(in super::super) fn parse_team_dispatch_args(args: &[String]) -> Result<TeamCommand, AppError> {
    let mut lanes = None;
    let mut owned_write_paths = Vec::new();
    let mut failed_lane = None;
    let mut failure_reason = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--lanes" => {
                if lanes.is_some() {
                    return Err(AppError::usage(
                        "team dispatch의 --lanes 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team dispatch는 --lanes <count> 값이 필요합니다.",
                    ));
                };
                lanes = Some(parse_positive_u32(value, "lanes")?);
                index += 2;
            }
            "--write-owner" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team dispatch는 --write-owner <lane:path> 값이 필요합니다.",
                    ));
                };
                if value.starts_with("--") {
                    return Err(AppError::usage(
                        "team dispatch는 --write-owner <lane:path> 값이 필요합니다.",
                    ));
                }
                let (lane, path) = parse_write_owner_for(value, "team dispatch")?;
                owned_write_paths.push((lane, path));
                index += 2;
            }
            "--failed-lane" => {
                if failed_lane.is_some() {
                    return Err(AppError::usage(
                        "team dispatch의 --failed-lane 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team dispatch는 --failed-lane <lane> 값이 필요합니다.",
                    ));
                };
                failed_lane = Some(parse_positive_u32(value, "failed-lane")?);
                index += 2;
            }
            "--failure" => {
                if failure_reason.is_some() {
                    return Err(AppError::usage(
                        "team dispatch의 --failure 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                index += 1;
                let start = index;
                while index < args.len() && !args[index].starts_with("--") {
                    index += 1;
                }
                if start == index {
                    return Err(AppError::usage(
                        "team dispatch는 --failure <reason> 값이 필요합니다.",
                    ));
                }
                failure_reason = Some(args[start..index].join(" "));
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 team dispatch 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let lanes = lanes
        .ok_or_else(|| AppError::usage("team dispatch는 --lanes <count> 형식이 필요합니다."))?;
    if owned_write_paths.is_empty() {
        return Err(AppError::usage(
            "team dispatch는 최소 하나의 --write-owner <lane:path> 값이 필요합니다.",
        ));
    }
    if let Some((lane, _)) = owned_write_paths.iter().find(|(lane, _)| *lane > lanes) {
        return Err(AppError::usage(format!(
            "team dispatch의 --write-owner lane {lane}은 --lanes {lanes} 값을 넘을 수 없습니다."
        )));
    }
    if failure_reason.is_some() && failed_lane.is_none() {
        return Err(AppError::usage(
            "team dispatch의 --failure는 --failed-lane <lane>과 함께 사용해야 합니다.",
        ));
    }

    Ok(TeamCommand::Dispatch {
        lanes,
        owned_write_paths,
        failed_lane,
        failure_reason,
    })
}

pub(in super::super) fn parse_team_governor_args(args: &[String]) -> Result<TeamCommand, AppError> {
    let mut lanes = None;
    let mut context_tokens = None;
    let mut context_limit = None;
    let mut model_tier = ModelTier::Small;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--lanes" => {
                if lanes.is_some() {
                    return Err(AppError::usage(
                        "team governor의 --lanes 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team governor는 --lanes <count> 값이 필요합니다.",
                    ));
                };
                lanes = Some(parse_positive_u32(value, "lanes")?);
                index += 2;
            }
            "--context-tokens" => {
                if context_tokens.is_some() {
                    return Err(AppError::usage(
                        "team governor의 --context-tokens 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team governor는 --context-tokens <tokens> 값이 필요합니다.",
                    ));
                };
                context_tokens = Some(parse_positive_u32(value, "context-tokens")?);
                index += 2;
            }
            "--context-limit" => {
                if context_limit.is_some() {
                    return Err(AppError::usage(
                        "team governor의 --context-limit 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team governor는 --context-limit <tokens> 값이 필요합니다.",
                    ));
                };
                context_limit = Some(parse_positive_u32(value, "context-limit")?);
                index += 2;
            }
            "--model-tier" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "team governor는 --model-tier <small|standard|large> 값이 필요합니다.",
                    ));
                };
                model_tier = ModelTier::parse(value).ok_or_else(|| {
                    AppError::usage(
                        "team governor의 --model-tier 값은 small, standard, large 중 하나여야 합니다.",
                    )
                })?;
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 team governor 옵션입니다: {unknown}"
                )));
            }
        }
    }

    Ok(TeamCommand::Governor {
        lanes: lanes
            .ok_or_else(|| AppError::usage("team governor는 --lanes <count> 형식이 필요합니다."))?,
        context_tokens: context_tokens.ok_or_else(|| {
            AppError::usage("team governor는 --context-tokens <tokens> 형식이 필요합니다.")
        })?,
        context_limit,
        model_tier,
    })
}
