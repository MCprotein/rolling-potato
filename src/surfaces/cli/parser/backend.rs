use super::*;
use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;

pub(super) fn parse_backend_start(args: &[String]) -> Result<BackendCommand, AppError> {
    let mut model_path = None;
    let mut ctx_size = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--model" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend start는 --model <path> 값이 필요합니다.",
                    ));
                };
                if model_path.is_some() {
                    return Err(AppError::usage(
                        "backend start의 --model 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                model_path = Some(value.clone());
                index += 2;
            }
            "--ctx-size" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend start는 --ctx-size <tokens> 값이 필요합니다.",
                    ));
                };
                if ctx_size.is_some() {
                    return Err(AppError::usage(
                        "backend start의 --ctx-size 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                ctx_size = Some(parse_positive_u32(value, "ctx-size")?);
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 backend start 옵션입니다: {unknown}"
                )));
            }
        }
    }

    Ok(BackendCommand::Start {
        model_path,
        ctx_size,
    })
}

pub(super) fn parse_backend_chat(args: &[String]) -> Result<BackendCommand, AppError> {
    let mut prompt = None;
    let mut max_tokens = None;
    let mut stream = false;
    let mut timeout_ms = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--prompt" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --prompt <text> 값이 필요합니다.",
                    ));
                };
                if prompt.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --prompt 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                prompt = Some(value.clone());
                index += 2;
            }
            "--max-tokens" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --max-tokens <tokens> 값이 필요합니다.",
                    ));
                };
                if max_tokens.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --max-tokens 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                max_tokens = Some(parse_positive_u32(value, "max-tokens")?);
                index += 2;
            }
            "--stream" => {
                if stream {
                    return Err(AppError::usage(
                        "backend chat의 --stream 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                stream = true;
                index += 1;
            }
            "--timeout-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --timeout-ms <ms> 값이 필요합니다.",
                    ));
                };
                if timeout_ms.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --timeout-ms 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let value = parse_positive_u32(value, "timeout-ms")?;
                if value > MAX_CHAT_TIMEOUT_MS {
                    return Err(AppError::usage(format!(
                        "backend chat timeout은 1..={} ms 범위여야 합니다.",
                        MAX_CHAT_TIMEOUT_MS
                    )));
                }
                timeout_ms = Some(value);
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 backend chat 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(prompt) = prompt else {
        return Err(AppError::usage(
            "backend chat은 --prompt <text> 형식이 필요합니다.",
        ));
    };

    Ok(BackendCommand::Chat {
        prompt,
        max_tokens,
        stream,
        timeout_ms,
    })
}
