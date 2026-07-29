use std::path::Path;

use crate::foundation::error::AppError;

use super::{push_unique, validate_component_name};

pub(crate) struct ParsedCodexSkill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) instructions: String,
}

pub(crate) struct ParsedClaudeInstruction {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) instructions: String,
}

pub(crate) fn parse_codex_skill(text: &str, path: &Path) -> Result<ParsedCodexSkill, AppError> {
    let normalized = text.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: SKILL.md YAML frontmatter가 없습니다.",
            path.display()
        )));
    };
    let Some((frontmatter, instructions)) = rest.split_once("\n---\n") else {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: SKILL.md YAML frontmatter 종료 marker가 없습니다.",
            path.display()
        )));
    };
    let field = |name: &str| {
        frontmatter
            .lines()
            .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == name))
            .map(|(_, value)| unquote_yaml_scalar(value.trim()))
            .filter(|value| !value.is_empty())
    };
    let name = field("name").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: frontmatter name이 없습니다.",
            path.display()
        ))
    })?;
    validate_component_name(&name, "skill")?;
    let description = field("description").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: frontmatter description이 없습니다.",
            path.display()
        ))
    })?;
    let instructions = instructions.trim().to_string();
    if instructions.is_empty() {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: instruction body가 비어 있습니다.",
            path.display()
        )));
    }
    Ok(ParsedCodexSkill {
        name,
        description,
        instructions,
    })
}

pub(crate) fn parse_claude_instruction(
    text: &str,
    path: &Path,
    invocation_name: &str,
) -> Result<ParsedClaudeInstruction, AppError> {
    validate_component_name(invocation_name, "skill")?;
    if contains_claude_dynamic_shell(text) {
        return Err(AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: Claude Code 동적 shell 삽입은 rpotato read-only adapter에서 지원하지 않습니다.",
            path.display()
        )));
    }
    let normalized = text.replace("\r\n", "\n");
    let (frontmatter, body) = split_optional_frontmatter(&normalized, path)?;
    let description = yaml_scalar_field(frontmatter, "description")
        .or_else(|| first_markdown_paragraph(body))
        .ok_or_else(|| {
            AppError::blocked(format!(
                "plugin instruction 실행 차단\n- path: {}\n- 이유: description 또는 instruction paragraph가 없습니다.",
                path.display()
            ))
        })?;
    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return Err(AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: instruction body가 비어 있습니다.",
            path.display()
        )));
    }
    Ok(ParsedClaudeInstruction {
        name: invocation_name.to_string(),
        description,
        instructions,
    })
}

pub(crate) fn contains_claude_dynamic_shell(text: &str) -> bool {
    text.as_bytes().windows(2).enumerate().any(|(index, pair)| {
        pair == b"!`" && (index == 0 || text.as_bytes()[index - 1].is_ascii_whitespace())
    }) || text
        .lines()
        .any(|line| line.trim_start().starts_with("```!"))
}

pub(crate) fn claude_instruction_unsupported(text: &str, relative_path: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let frontmatter = normalized
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(frontmatter, _)| frontmatter)
        .unwrap_or_default();
    let mut unsupported = Vec::new();
    for field in [
        "when_to_use",
        "argument-hint",
        "arguments",
        "disable-model-invocation",
        "user-invocable",
        "allowed-tools",
        "disallowed-tools",
        "model",
        "effort",
        "context",
        "agent",
        "hooks",
        "paths",
        "shell",
    ] {
        if yaml_has_field(frontmatter, field) {
            push_unique(
                &mut unsupported,
                &format!("claude-frontmatter:{relative_path}:{field}"),
            );
        }
    }
    if normalized.contains("$ARGUMENTS")
        || normalized.contains("${CLAUDE_PLUGIN_ROOT}")
        || normalized.contains("${CLAUDE_PLUGIN_DATA}")
        || normalized.contains("${CLAUDE_PROJECT_DIR}")
        || normalized.contains("${CLAUDE_SESSION_ID}")
        || normalized.contains("${CLAUDE_EFFORT}")
        || normalized.contains("${CLAUDE_SKILL_DIR}")
        || normalized.contains("${user_config.")
        || contains_unescaped_positional_substitution(&normalized)
        || (yaml_has_field(frontmatter, "arguments")
            && contains_unescaped_named_substitution(&normalized))
    {
        push_unique(
            &mut unsupported,
            &format!("claude-template-substitution:{relative_path}"),
        );
    }
    unsupported
}

fn contains_unescaped_positional_substitution(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(2).enumerate().any(|(index, pair)| {
        pair[0] == b'$' && pair[1].is_ascii_digit() && (index == 0 || bytes[index - 1] != b'\\')
    })
}

fn contains_unescaped_named_substitution(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(2).enumerate().any(|(index, pair)| {
        pair[0] == b'$'
            && (pair[1].is_ascii_alphabetic() || pair[1] == b'_')
            && (index == 0 || bytes[index - 1] != b'\\')
    })
}

fn split_optional_frontmatter<'a>(
    text: &'a str,
    path: &Path,
) -> Result<(&'a str, &'a str), AppError> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok(("", text));
    };
    rest.split_once("\n---\n").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: YAML frontmatter 종료 marker가 없습니다.",
            path.display()
        ))
    })
}

fn yaml_scalar_field(frontmatter: &str, name: &str) -> Option<String> {
    frontmatter
        .lines()
        .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == name))
        .map(|(_, value)| unquote_yaml_scalar(value.trim()))
        .filter(|value| !value.is_empty() && value != "|" && value != ">")
}

fn yaml_has_field(frontmatter: &str, name: &str) -> bool {
    frontmatter.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(key, _)| key.trim() == name)
    })
}

fn first_markdown_paragraph(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect())
}

fn unquote_yaml_scalar(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}
