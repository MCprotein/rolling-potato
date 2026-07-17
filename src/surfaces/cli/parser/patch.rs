use super::*;

pub(super) fn parse_patch_preview(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut path = None;
    let mut find = None;
    let mut replace = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--path" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --path <path> 값이 필요합니다.",
                    ));
                };
                path = Some(value.clone());
                index += 2;
            }
            "--find" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --find <text> 값이 필요합니다.",
                    ));
                };
                find = Some(value.clone());
                index += 2;
            }
            "--replace" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --replace <text> 값이 필요합니다.",
                    ));
                };
                replace = Some(value.clone());
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch preview 옵션입니다: {unknown}"
                )));
            }
        }
    }

    Ok(PatchCommand::Preview {
        path: path.ok_or_else(|| AppError::usage("patch preview는 --path가 필요합니다."))?,
        find: find.ok_or_else(|| AppError::usage("patch preview는 --find가 필요합니다."))?,
        replace: replace
            .ok_or_else(|| AppError::usage("patch preview는 --replace가 필요합니다."))?,
    })
}

pub(super) fn parse_patch_approve(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut proposal_id = None;
    let mut token = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--token" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch approve는 --token <token> 값이 필요합니다.",
                    ));
                };
                token = Some(value.clone());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch approve 옵션입니다: {value}"
                )));
            }
            value => {
                if proposal_id.is_some() {
                    return Err(AppError::usage(
                        "patch approve proposal id는 하나만 지정할 수 있습니다.",
                    ));
                }
                proposal_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    let Some(proposal_id) = proposal_id else {
        return Err(AppError::usage(
            "patch approve에는 proposal id가 필요합니다.",
        ));
    };
    let Some(token) = token else {
        return Err(AppError::usage(
            "patch approve는 --token <token> 값이 필요합니다.",
        ));
    };

    Ok(PatchCommand::Approve {
        proposal_id,
        token,
        dry_run,
    })
}

pub(super) fn parse_patch_verify(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut proposal_id = None;
    let mut token = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--token" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch verify는 --token <token> 값이 필요합니다.",
                    ));
                };
                token = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch verify 옵션입니다: {value}"
                )));
            }
            value => {
                if proposal_id.is_some() {
                    return Err(AppError::usage(
                        "patch verify proposal id는 하나만 지정할 수 있습니다.",
                    ));
                }
                proposal_id = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(PatchCommand::Verify {
        proposal_id: proposal_id
            .ok_or_else(|| AppError::usage("patch verify에는 proposal id가 필요합니다."))?,
        token: token
            .ok_or_else(|| AppError::usage("patch verify는 --token <token> 값이 필요합니다."))?,
    })
}
