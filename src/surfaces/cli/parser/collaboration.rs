use super::*;

pub(super) fn parse_team_plan_args(args: &[String]) -> Result<TeamCommand, AppError> {
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

pub(super) fn parse_team_execute_args(args: &[String]) -> Result<TeamCommand, AppError> {
    Ok(TeamCommand::Execute {
        team_id: parse_team_id_args(args, "team execute")?,
    })
}

pub(super) fn parse_team_reconcile_args(args: &[String]) -> Result<TeamCommand, AppError> {
    Ok(TeamCommand::Reconcile {
        team_id: parse_team_id_args(args, "team reconcile")?,
    })
}

pub(super) fn parse_team_cancel_args(args: &[String]) -> Result<TeamCommand, AppError> {
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

pub(super) fn parse_team_admit_args(args: &[String]) -> Result<TeamCommand, AppError> {
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

pub(super) fn parse_subagent_launch_args(args: &[String]) -> Result<SubagentCommand, AppError> {
    let mut role = None;
    let mut task = None;
    let mut tools = Vec::new();
    let mut read_paths = Vec::new();
    let mut write_paths = Vec::new();
    let mut timeout_ms = None;
    let mut max_tokens = None;
    let mut index = 0;

    while index < args.len() {
        let flag = args[index].as_str();
        let Some(value) = args.get(index + 1) else {
            return Err(AppError::usage(format!(
                "subagent launch의 {flag} 옵션에는 값이 필요합니다."
            )));
        };
        if value.starts_with("--") {
            return Err(AppError::usage(format!(
                "subagent launch의 {flag} 옵션에는 값이 필요합니다."
            )));
        }
        match flag {
            "--role" => set_subagent_single_value(&mut role, value, flag)?,
            "--task" => set_subagent_single_value(&mut task, value, flag)?,
            "--tool" => tools.push(value.clone()),
            "--read" => read_paths.push(value.clone()),
            "--write" => write_paths.push(value.clone()),
            "--timeout-ms" => {
                if timeout_ms.is_some() {
                    return Err(AppError::usage(
                        "subagent launch의 --timeout-ms 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                timeout_ms = Some(parse_subagent_u32(value, "--timeout-ms")?);
            }
            "--max-tokens" => {
                if max_tokens.is_some() {
                    return Err(AppError::usage(
                        "subagent launch의 --max-tokens 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                max_tokens = Some(parse_subagent_u32(value, "--max-tokens")?);
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 subagent launch 옵션입니다: {unknown}"
                )));
            }
        }
        index += 2;
    }

    let role =
        role.ok_or_else(|| AppError::usage("subagent launch에는 --role <role> 값이 필요합니다."))?;
    let task =
        task.ok_or_else(|| AppError::usage("subagent launch에는 --task <text> 값이 필요합니다."))?;
    if tools.is_empty() || read_paths.is_empty() {
        return Err(AppError::usage(
            "subagent launch에는 최소 하나의 --tool <tool>과 --read <path>가 필요합니다.",
        ));
    }
    Ok(SubagentCommand::Launch {
        role,
        task,
        tools,
        read_paths,
        write_paths,
        timeout_ms,
        max_tokens,
    })
}

fn set_subagent_single_value(
    slot: &mut Option<String>,
    value: &str,
    flag: &str,
) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "subagent launch의 {flag} 옵션은 한 번만 지정할 수 있습니다."
        )));
    }
    *slot = Some(value.to_string());
    Ok(())
}

fn parse_subagent_u32(value: &str, flag: &str) -> Result<u32, AppError> {
    let parsed = value.parse::<u32>().map_err(|_| {
        AppError::usage(format!(
            "subagent launch의 {flag} 값은 양의 정수여야 합니다."
        ))
    })?;
    if parsed == 0 {
        return Err(AppError::usage(format!(
            "subagent launch의 {flag} 값은 1 이상이어야 합니다."
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_team_dispatch_args(args: &[String]) -> Result<TeamCommand, AppError> {
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

pub(super) fn parse_team_governor_args(args: &[String]) -> Result<TeamCommand, AppError> {
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

fn parse_write_owner_for(value: &str, command: &str) -> Result<(u32, String), AppError> {
    let Some((lane, path)) = value.split_once(':') else {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner 값은 <lane:path> 형식이어야 합니다."
        )));
    };
    let lane = lane.parse::<u32>().map_err(|_| {
        AppError::usage(format!(
            "{command}의 --write-owner lane은 양의 정수여야 합니다."
        ))
    })?;
    if lane == 0 {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner lane은 1 이상이어야 합니다."
        )));
    }
    if path.trim().is_empty() {
        return Err(AppError::usage(format!(
            "{command}의 --write-owner path는 비어 있을 수 없습니다."
        )));
    }
    Ok((lane, path.to_string()))
}
