//! Sensitive-value detection and deterministic text redaction.

pub fn contains_sensitive_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let normalized = lower
        .chars()
        .filter(|character| !matches!(character, '"' | '\'' | '\\'))
        .collect::<String>();
    [
        "api_key",
        "apikey",
        "authorization",
        "password",
        "secret",
        "token",
    ]
    .iter()
    .any(|key| contains_sensitive_assignment(&normalized, key))
        || normalized.split_whitespace().any(|part| part == "bearer")
        || contains_bounded_prefix(&lower, "sk-", 8)
        || contains_bounded_prefix(&lower, "ghp_", 8)
        || contains_bounded_prefix(&lower, "github_pat_", 8)
        || value.contains("-----BEGIN PRIVATE KEY-----")
        || value
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| {
                part.len() == 20
                    && part.starts_with("AKIA")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            })
}

fn contains_sensitive_assignment(value: &str, key: &str) -> bool {
    value.match_indices(key).any(|(index, _)| {
        if index > 0 && value.as_bytes()[index - 1].is_ascii_alphanumeric() {
            return false;
        }
        let tail = value[index + key.len()..].trim_start();
        let Some(tail) = tail.strip_prefix('=').or_else(|| tail.strip_prefix(':')) else {
            return false;
        };
        tail.trim_start()
            .chars()
            .next()
            .is_some_and(|character| !matches!(character, ',' | '}' | ']'))
    })
}

fn contains_bounded_prefix(value: &str, prefix: &str, minimum_suffix: usize) -> bool {
    value.match_indices(prefix).any(|(index, _)| {
        let boundary = index == 0 || !value.as_bytes()[index - 1].is_ascii_alphanumeric();
        if !boundary {
            return false;
        }
        value[index + prefix.len()..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
            .count()
            >= minimum_suffix
    })
}

pub fn redact_text(value: &str) -> String {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let follows_bearer = index > 0 && parts[index - 1].eq_ignore_ascii_case("bearer");
            if contains_sensitive_text(part)
                || part.eq_ignore_ascii_case("bearer")
                || follows_bearer
            {
                "[REDACTED]".to_string()
            } else {
                (*part).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
