//! Exact argv parsing and command policy classification.

use crate::foundation::error::AppError;
use crate::foundation::serialization::{self as strict_json, Value};

use super::types::{ActionKind, Decision, ParsedCommand, PolicyDecision};

pub fn classify_command(command: &str) -> Result<PolicyDecision, AppError> {
    let parsed = parse_exact_argv(command)?;
    let first = parsed.argv[0].as_str();
    if matches!(
        first,
        "rm" | "sh" | "bash" | "zsh" | "python" | "python3" | "mkfs" | "dd"
    ) || matches!(parsed.argv.as_slice(), [a, b, ..] if a == "git" && ((b == "reset" && parsed.argv.iter().any(|v| v == "--hard")) || b == "checkout"))
    {
        return Ok(PolicyDecision::new(
            Decision::Deny,
            ActionKind::RunCommand,
            "destructive-or-interpreter",
            "shell/interpreter/destructive command는 차단합니다.",
            "차단",
        ));
    }
    if matches!(first, "curl" | "wget")
        || matches!(parsed.argv.as_slice(), [a, b, ..] if (a == "git" && b == "clone") || (a == "cargo" && b == "add"))
    {
        return Ok(PolicyDecision::new(
            Decision::Ask,
            ActionKind::NetworkDownload,
            "network-or-dependency",
            "network/download/dependency 변경은 승인 prompt가 필요합니다.",
            "사용자 승인 필요",
        ));
    }
    if is_general_read_only(&parsed.argv) || validate_patch_verification_argv(&parsed.argv).is_ok()
    {
        return Ok(PolicyDecision::new(
            Decision::Allow,
            ActionKind::RunCommand,
            "read-only-or-verification",
            "읽기/검증 명령으로 분류되어 승인 없이 실행 가능",
            "불필요",
        ));
    }

    Ok(PolicyDecision::new(
        Decision::Ask,
        ActionKind::RunCommand,
        "unknown-side-effect",
        "side effect 여부가 확실하지 않아 승인 prompt가 필요합니다.",
        "사용자 승인 필요",
    ))
}

pub fn parse_patch_verification(command: &str) -> Result<ParsedCommand, AppError> {
    let parsed = parse_exact_argv(command)?;
    validate_patch_verification_argv(&parsed.argv)?;
    Ok(parsed)
}

/// Parses the deliberately small command language exposed to the local agent.
///
/// The returned argv is safe to pass directly to `std::process::Command`; the
/// caller must still resolve every path against the canonical project root.
pub(crate) fn parse_local_read_only_command(input: &str) -> Result<ParsedCommand, AppError> {
    let Value::Array(values) = strict_json::parse_value(input.trim(), "local command argv")? else {
        return Err(AppError::blocked(
            "local read-only command 차단\n- 이유: command는 JSON string argv array여야 합니다.",
        ));
    };
    let argv = values
        .into_iter()
        .map(|value| match value {
            Value::String(value)
                if !value.is_empty()
                    && !value.contains('\0')
                    && !value.chars().any(is_shell_metacharacter) =>
            {
                Ok(value)
            }
            _ => Err(AppError::blocked(
                "local read-only command 차단\n- 이유: argv는 비어 있지 않은 JSON string만 포함해야 합니다.",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parsed = ParsedCommand {
        display: argv.join(" "),
        argv,
    };
    validate_local_read_only_argv(&parsed.argv)?;
    Ok(parsed)
}

fn is_shell_metacharacter(ch: char) -> bool {
    matches!(
        ch,
        ';' | '|' | '&' | '<' | '>' | '`' | '$' | '\n' | '\r' | '\'' | '"' | '(' | ')'
    )
}

fn validate_local_read_only_argv(argv: &[String]) -> Result<(), AppError> {
    let valid = match argv {
        [command] => matches!(command.as_str(), "pwd" | "ls"),
        [git, status, options @ ..] if git == "git" && status == "status" => {
            options.is_empty()
                || matches!(options, [one] if one == "--short")
                || matches!(options, [one] if one == "--branch")
                || matches!(options, [one, two] if one == "--short" && two == "--branch")
                || matches!(options, [one, two] if one == "--branch" && two == "--short")
        }
        [git, diff, mode, rest @ ..]
            if git == "git"
                && diff == "diff"
                && matches!(mode.as_str(), "--stat" | "--name-only") =>
        {
            rest.is_empty()
                || (rest.first().is_some_and(|value| value == "--")
                    && rest.len() > 1
                    && rest[1..].iter().all(|path| valid_relative_token(path)))
        }
        [rg, files] if rg == "rg" && files == "--files" => true,
        [rg, files, separator, directory]
            if rg == "rg" && files == "--files" && separator == "--" =>
        {
            valid_relative_token(directory)
        }
        [rg, line_numbers, rest @ ..] if rg == "rg" && line_numbers == "-n" => {
            validate_rg_search(rest)
        }
        [command, count, number, separator, file]
            if matches!(command.as_str(), "head" | "tail")
                && count == "-n"
                && separator == "--" =>
        {
            number
                .parse::<u16>()
                .is_ok_and(|number| (1..=200).contains(&number))
                && valid_relative_token(file)
        }
        [wc, lines, separator, files @ ..] if wc == "wc" && lines == "-l" && separator == "--" => {
            !files.is_empty() && files.iter().all(|path| valid_relative_token(path))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(AppError::blocked(
            "local read-only command 차단\n- 이유: 허용된 정확한 command 문법과 일치하지 않습니다.",
        ))
    }
}

fn validate_rg_search(rest: &[String]) -> bool {
    let (case_flag, rest) = match rest {
        [flag, rest @ ..] if flag == "-i" => (true, rest),
        rest => (false, rest),
    };
    let _ = case_flag;
    matches!(rest, [fixed, separator, literal, paths @ ..]
        if fixed == "-F"
            && separator == "--"
            && !literal.is_empty()
            && paths.iter().all(|path| valid_relative_token(path)))
}

fn valid_relative_token(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.starts_with('\\')
        && !value.contains('\\')
        && (value == "."
            || !value
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | "..")))
}

fn parse_exact_argv(command: &str) -> Result<ParsedCommand, AppError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage("검사할 command가 필요합니다."));
    }
    if trimmed.chars().any(|ch| {
        matches!(
            ch,
            ';' | '|' | '&' | '<' | '>' | '`' | '$' | '\n' | '\r' | '"' | '\'' | '(' | ')'
        )
    }) {
        return Err(AppError::blocked(
            "command 검증 차단\n- 이유: shell metacharacter 또는 chaining은 허용하지 않습니다.",
        ));
    }
    let argv = trimmed
        .split_ascii_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if argv
        .first()
        .is_some_and(|arg| arg.contains('/') || arg.contains('\\'))
    {
        return Err(AppError::blocked(
            "command 검증 차단\n- 이유: path-like executable/argument는 허용하지 않습니다.",
        ));
    }
    Ok(ParsedCommand {
        display: argv.join(" "),
        argv,
    })
}

fn validate_patch_verification_argv(argv: &[String]) -> Result<(), AppError> {
    if argv == ["pwd"] {
        return Ok(());
    }
    if argv.first().map(String::as_str) != Some("cargo") || argv.len() < 2 {
        return Err(AppError::blocked(
            "patch verification 차단\n- 이유: pwd 또는 제한된 cargo verification만 허용합니다.",
        ));
    }
    let subcommand = argv[1].as_str();
    if !matches!(subcommand, "test" | "check" | "fmt" | "clippy") {
        return Err(AppError::blocked(
            "patch verification 차단\n- 이유: cargo test/check/fmt/clippy만 허용합니다.",
        ));
    }
    if subcommand == "fmt" {
        if argv != ["cargo", "fmt", "--", "--check"] {
            return Err(AppError::blocked(
                "patch verification 차단\n- 이유: cargo fmt는 정확히 `cargo fmt -- --check`만 허용합니다.",
            ));
        }
        return Ok(());
    }
    let mut index = 2;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if matches!(arg, "--manifest-path" | "--package" | "-p")
            || arg.starts_with("--manifest-path=")
            || arg.starts_with("--package=")
        {
            return Err(AppError::blocked(
                "patch verification 차단\n- 이유: 외부 manifest/package 지정은 허용하지 않습니다.",
            ));
        }
        let takes_value = matches!(arg, "--bin" | "--test" | "--example" | "--features");
        let allowed = matches!(
            arg,
            "--locked"
                | "--all-targets"
                | "--tests"
                | "--bins"
                | "--lib"
                | "--examples"
                | "--release"
                | "--check"
                | "--no-default-features"
                | "--bin"
                | "--test"
                | "--example"
                | "--features"
        );
        if !allowed {
            return Err(AppError::blocked(format!(
                "patch verification 차단\n- 이유: 허용되지 않은 cargo argument: {arg}"
            )));
        }
        if takes_value {
            index += 1;
            let Some(value) = argv.get(index) else {
                return Err(AppError::blocked(
                    "patch verification 차단\n- 이유: cargo argument 값이 누락되었습니다.",
                ));
            };
            if value.is_empty()
                || !value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ','))
            {
                return Err(AppError::blocked("patch verification 차단\n- 이유: cargo argument 값이 안전한 identifier가 아닙니다."));
            }
        }
        index += 1;
    }
    Ok(())
}

fn is_general_read_only(argv: &[String]) -> bool {
    matches!(argv, [one] if one == "pwd" || one == "ls" || one == "git")
        || matches!(argv, [one, two] if (one == "git" && matches!(two.as_str(), "status" | "diff")) || (one == "cargo" && matches!(two.as_str(), "test" | "check" | "clippy")))
        || matches!(
            argv.first().map(String::as_str),
            Some("rg" | "head" | "tail" | "wc")
        )
}

#[cfg(test)]
mod local_command_tests {
    use super::*;

    #[test]
    fn accepts_only_the_local_read_only_grammar() {
        for command in [
            r#"["pwd"]"#,
            r#"["ls"]"#,
            r#"["git","status"]"#,
            r#"["git","status","--short","--branch"]"#,
            r#"["git","status","--branch","--short"]"#,
            r#"["git","diff","--stat"]"#,
            r#"["git","diff","--name-only","--","src/lib.rs","docs"]"#,
            r#"["rg","--files"]"#,
            r#"["rg","--files","--","."]"#,
            r#"["rg","-n","-F","--","literal phrase","src/lib.rs"]"#,
            r#"["rg","-n","-i","-F","--","Needle","src"]"#,
            r#"["head","-n","200","--","src/lib.rs"]"#,
            r#"["tail","-n","1","--","README.md"]"#,
            r#"["wc","-l","--","src/lib.rs","README.md"]"#,
        ] {
            assert!(parse_local_read_only_command(command).is_ok(), "{command}");
        }
    }

    #[test]
    fn rejects_options_traversal_and_shell_syntax() {
        for command in [
            r#"["git","status","--porcelain"]"#,
            r#"["git","status","--short","--short"]"#,
            r#"["git","diff"]"#,
            r#"["git","diff","--stat","--","../outside"]"#,
            r#"["rg","needle"]"#,
            r#"["rg","-n","-F","needle"]"#,
            r#"["rg","-n","-F","--","needle","../outside"]"#,
            r#"["head","-n","0","--","README.md"]"#,
            r#"["head","-n","201","--","README.md"]"#,
            r#"["wc","-l","--"]"#,
            r#"["sh","-c","pwd"]"#,
            r#"["/bin/pwd"]"#,
            r#"["rg","-n","-F","--","needle;id"]"#,
            r#"{"command":"pwd"}"#,
        ] {
            assert!(parse_local_read_only_command(command).is_err(), "{command}");
        }
    }
}
