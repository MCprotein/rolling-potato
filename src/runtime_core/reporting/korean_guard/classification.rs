#[derive(Debug, Default)]
pub(super) struct OutsideTextClassification {
    pub(super) forbidden: bool,
    pub(super) has_hangul: bool,
    pub(super) language_neutral: bool,
}

pub(super) fn classify_outside_text(text: &str) -> OutsideTextClassification {
    let mut result = OutsideTextClassification::default();
    for raw in text.lines() {
        let trimmed = raw.trim();
        let line = strip_inline_code(trimmed);
        if line.chars().any(is_hiragana_katakana_or_han) {
            result.forbidden = true;
            return result;
        }
        if let Some(value) = runtime_field_value(&line) {
            let token_like = value
                .chars()
                .all(|character| !character.is_control() && !character.is_whitespace());
            if !token_like && has_excessive_foreign_prose(value) {
                result.forbidden = true;
                return result;
            }
            result.has_hangul |= value.chars().any(is_hangul);
            result.language_neutral = true;
            continue;
        }
        result.has_hangul |= line.chars().any(is_hangul);
        let foreign_words = foreign_word_count(&line);
        if has_excessive_foreign_prose(&line) {
            result.forbidden = true;
            return result;
        }
        result.language_neutral |= !line.is_empty()
            && !result.has_hangul
            && foreign_words == 0
            && line.chars().any(|ch| !ch.is_whitespace());
    }
    result
}

pub(super) fn runtime_field_value(line: &str) -> Option<&str> {
    let (label, value) = line.strip_prefix("- ")?.split_once(": ")?;
    let label = label.to_ascii_lowercase();
    let known = [
        "path",
        "code",
        "kind",
        "effect",
        "retry",
        "intent",
        "hash",
        "sha",
        "token",
        "command",
        "stdout",
        "stderr",
        "record",
        "event",
        "status",
        "phase",
        "pointer",
        "revision",
        "error",
        "exit code",
    ]
    .iter()
    .any(|marker| label == *marker || label.contains(marker));
    (known || label.ends_with(" id") || label.contains("파일") || label.contains("경로"))
        .then_some(value)
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn is_safe_literal(line: &str) -> bool {
    !line.is_empty()
        && !line.chars().any(|character| character.is_whitespace())
        && (line.starts_with("https://")
            || line.starts_with("http://")
            || line.contains('/')
            || line.contains('\\'))
}

pub(super) fn strip_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    for ch in line.chars() {
        if ch == '`' {
            code = !code;
        } else if !code {
            out.push(ch);
        }
    }
    out
}

fn has_excessive_foreign_prose(text: &str) -> bool {
    let longest_foreign_span = text
        .split(is_hangul)
        .map(foreign_word_count)
        .max()
        .unwrap_or_default();
    if longest_foreign_span < 3 {
        return false;
    }
    if longest_foreign_span <= 8 && is_release_title_context(text) {
        return false;
    }
    true
}

fn is_release_title_context(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_release_label = text.contains("릴리스")
        || text.contains("버전")
        || lower.contains("release")
        || lower.contains("version");
    let has_version = lower
        .split(|character: char| character.is_whitespace() || character == '-')
        .any(|token| {
            token
                .strip_prefix('v')
                .and_then(|version| version.chars().next())
                .is_some_and(|character| character.is_ascii_digit())
        });
    has_release_label && has_version
}

fn foreign_word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|token| !is_path_or_url_token(token))
        .flat_map(|token| token.split(|ch: char| !ch.is_ascii_alphanumeric()))
        .filter(|word| {
            word.len() > 1
                && word
                    .chars()
                    .any(|character| character.is_ascii_alphabetic())
                && !allowed_ascii(word)
        })
        .count()
}

fn is_path_or_url_token(token: &str) -> bool {
    let token = token.trim_matches(|character: char| {
        matches!(
            character,
            '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '"' | '\'' | '`'
        )
    });
    if token.starts_with("https://") || token.starts_with("http://") {
        return true;
    }
    let separator = if token.contains('/') {
        '/'
    } else if token.contains('\\') {
        '\\'
    } else {
        return false;
    };
    token
        .rsplit(separator)
        .next()
        .is_some_and(|name| name.contains('.') && name.len() > 2)
}

fn is_hangul(ch: char) -> bool {
    matches!(ch as u32, 0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F)
}

fn is_hiragana_katakana_or_han(ch: char) -> bool {
    matches!(ch as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF)
}

fn allowed_ascii(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "cargo"
            | "test"
            | "check"
            | "fmt"
            | "clippy"
            | "pwd"
            | "workflow"
            | "action"
            | "proposal"
            | "evidence"
            | "stop"
            | "gate"
            | "sha"
            | "id"
            | "path"
            | "token"
            | "approval"
            | "source"
            | "hash"
            | "phase"
            | "backend"
            | "model"
            | "runtime"
            | "ledger"
            | "none"
            | "failed"
            | "complete"
            | "verified"
            | "pending"
            | "error"
            | "status"
            | "result"
            | "validation"
            | "gap"
            | "diff"
            | "apply"
            | "rollback"
            | "command"
            | "revision"
            | "current"
            | "pointer"
            | "artifact"
            | "passed"
            | "false"
            | "true"
            | "rpotato"
            | "json"
            | "sqlite"
            | "agent"
            | "loop"
            | "approve"
            | "preview"
            | "canonical"
            | "checkpoint"
            | "verification"
            | "original"
            | "sha256"
            | "side"
            | "effect"
            | "execute"
            | "executed"
            | "non"
            | "executable"
            | "record"
            | "fail"
            | "closed"
            | "failure"
            | "reason"
            | "rolled"
            | "back"
    )
}
