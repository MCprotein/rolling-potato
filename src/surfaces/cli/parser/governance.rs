use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{
    EvidenceCommand, HooksCommand, PolicyCommand, PolicyPathMode, SkillCommand,
};

use super::parse_request;

pub(super) fn parse_evidence(args: &[String]) -> Result<EvidenceCommand, AppError> {
    match args {
        [action, pointer] if action == "validate" => Ok(EvidenceCommand::Validate {
            pointer: pointer.clone(),
        }),
        [action, ..] if action == "validate" => Err(AppError::usage(
            "evidence validate에는 artifact pointer가 필요합니다.",
        )),
        _ => Err(AppError::usage("evidence 명령은 validate만 허용합니다.")),
    }
}

pub(super) fn parse_skill(args: &[String]) -> Result<SkillCommand, AppError> {
    match args {
        [action] if action == "list" => Ok(SkillCommand::List),
        [action, id, rest @ ..] if action == "run" => Ok(SkillCommand::Run {
            id: id.clone(),
            request: parse_request(rest, "skill run")?,
        }),
        [action, ..] if action == "run" => Err(AppError::usage(
            "skill run에는 skill id와 요청이 필요합니다. 예: rpotato skill run fix-test \"테스트 실패를 고쳐줘\"",
        )),
        _ => Err(AppError::usage(
            "skill 명령은 list 또는 run만 허용합니다.",
        )),
    }
}

pub(super) fn parse_policy(args: &[String]) -> Result<PolicyCommand, AppError> {
    match args {
        [action] if action == "schema" => Ok(PolicyCommand::Schema),
        [action, rest @ ..] if action == "check-command" => Ok(PolicyCommand::CheckCommand {
            command: parse_request(rest, "policy check-command")?,
        }),
        [action, flag, path] if action == "check-path" => {
            let mode = match flag.as_str() {
                "--read" => PolicyPathMode::Read,
                "--write" => PolicyPathMode::Write,
                _ => {
                    return Err(AppError::usage(
                        "policy check-path는 --read 또는 --write만 허용합니다.",
                    ));
                }
            };
            Ok(PolicyCommand::CheckPath {
                mode,
                path: path.clone(),
            })
        }
        [action, rest @ ..] if action == "redact" => Ok(PolicyCommand::Redact {
            text: parse_request(rest, "policy redact")?,
        }),
        _ => Err(AppError::usage(
            "policy 명령은 schema, check-command, check-path, redact만 허용합니다.",
        )),
    }
}

pub(super) fn parse_hooks(args: &[String]) -> Result<HooksCommand, AppError> {
    match args {
        [action] if action == "list" => Ok(HooksCommand::List),
        [action, rest @ ..] if action == "validate-result" => Ok(HooksCommand::ValidateResult {
            json: parse_request(rest, "hooks validate-result")?,
        }),
        _ => Err(AppError::usage(
            "hooks 명령은 list 또는 validate-result만 허용합니다.",
        )),
    }
}
