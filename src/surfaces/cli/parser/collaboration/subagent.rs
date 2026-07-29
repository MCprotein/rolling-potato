use super::{AppError, SubagentCommand};

pub(in super::super) fn parse_subagent_launch_args(
    args: &[String],
) -> Result<SubagentCommand, AppError> {
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
            "--role" => set_single_value(&mut role, value, flag)?,
            "--task" => set_single_value(&mut task, value, flag)?,
            "--tool" => tools.push(value.clone()),
            "--read" => read_paths.push(value.clone()),
            "--write" => write_paths.push(value.clone()),
            "--timeout-ms" => {
                if timeout_ms.is_some() {
                    return Err(AppError::usage(
                        "subagent launch의 --timeout-ms 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                timeout_ms = Some(parse_positive_u32(value, "--timeout-ms")?);
            }
            "--max-tokens" => {
                if max_tokens.is_some() {
                    return Err(AppError::usage(
                        "subagent launch의 --max-tokens 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                max_tokens = Some(parse_positive_u32(value, "--max-tokens")?);
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

fn set_single_value(slot: &mut Option<String>, value: &str, flag: &str) -> Result<(), AppError> {
    if slot.is_some() {
        return Err(AppError::usage(format!(
            "subagent launch의 {flag} 옵션은 한 번만 지정할 수 있습니다."
        )));
    }
    *slot = Some(value.to_string());
    Ok(())
}

fn parse_positive_u32(value: &str, flag: &str) -> Result<u32, AppError> {
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
