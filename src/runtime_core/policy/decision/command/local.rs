//! Exact JSON argv grammar exposed to the project-local agent.

use crate::foundation::error::AppError;
use crate::foundation::serialization::{self as strict_json, Value};

use super::super::types::ParsedCommand;

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
    let rest = match rest {
        [flag, rest @ ..] if flag == "-i" => rest,
        rest => rest,
    };
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

#[cfg(test)]
mod tests {
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
