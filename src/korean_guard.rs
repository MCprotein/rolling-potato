const FAILURE: &str =
    "응답 언어 검증에 실패했습니다. 출력이 한국어 기준을 만족하지 않아 결과를 표시하지 않았습니다.";

pub fn guard_or_failure(text: &str) -> String {
    guard_with_regeneration(text, || stricter_projection(text))
}

pub fn guard_with_regeneration<F>(text: &str, regenerate: F) -> String
where
    F: FnOnce() -> String,
{
    if validate(text) {
        return text.to_string();
    }
    let retry = regenerate();
    if validate(&retry) {
        retry
    } else {
        FAILURE.to_string()
    }
}

pub fn validate(text: &str) -> bool {
    let mut fenced = false;
    let mut saw_hangul = false;
    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if is_relaxed_technical_line(trimmed) {
            continue;
        }
        let line = strip_inline_code(trimmed);
        if line.chars().any(is_hiragana_katakana_or_han) {
            return false;
        }
        saw_hangul |= line.chars().any(is_hangul);
        if !line.is_empty()
            && line.chars().any(|ch| ch.is_ascii_alphabetic())
            && line
                .chars()
                .all(|ch| ch.is_ascii() || ch.is_ascii_whitespace())
            && line
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .filter(|word| !word.is_empty())
                .any(|word| !allowed_ascii(word))
        {
            return false;
        }
        let words = line
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|word| word.len() > 1 && !allowed_ascii(word))
            .count();
        if words >= 4 {
            return false;
        }
    }
    saw_hangul
}

fn stricter_projection(text: &str) -> String {
    text.lines()
        .filter(|line| line.chars().any(is_hangul))
        .filter(|line| !line.chars().any(is_hiragana_katakana_or_han))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_relaxed_technical_line(line: &str) -> bool {
    let Some((label, _)) = line
        .strip_prefix("- ")
        .and_then(|line| line.split_once(":"))
    else {
        return false;
    };
    let label = label.to_ascii_lowercase();
    [
        "path",
        " id",
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
    .any(|marker| label == *marker || label.contains(marker))
}

fn strip_inline_code(line: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_korean_passes() {
        assert!(validate("작업이 안전하게 완료되었습니다."));
    }
    #[test]
    fn english_code_block_passes() {
        assert!(validate(
            "검증 결과입니다.\n```text\nEnglish output here\n```"
        ));
    }
    #[test]
    fn file_path_passes() {
        assert!(validate("파일 `src/main.rs`를 확인했습니다."));
    }
    #[test]
    fn english_explanation_blocks() {
        assert!(!validate("This is a full English explanation."));
    }
    #[test]
    fn chinese_sentence_blocks() {
        assert!(!validate("작업 결과: 这是中文句子。"));
    }
    #[test]
    fn japanese_sentence_blocks() {
        assert!(!validate("작업 결과: これは日本語です。"));
    }
    #[test]
    fn regeneration_can_pass() {
        assert_eq!(
            guard_with_regeneration("This is invalid English text.", || {
                "다시 생성한 한국어 결과입니다.".into()
            }),
            "다시 생성한 한국어 결과입니다."
        );
    }
    #[test]
    fn regeneration_fails_closed() {
        assert_eq!(
            guard_with_regeneration("This is invalid English text.", || {
                "Still invalid English text here.".into()
            }),
            FAILURE
        );
    }
    #[test]
    fn short_english_heading_is_blocked() {
        assert!(!validate("Summary\n작업이 완료되었습니다."));
    }
    #[test]
    fn empty_regeneration_is_not_accepted() {
        assert_eq!(guard_with_regeneration("Summary", String::new), FAILURE);
    }
    #[test]
    fn runtime_projection_removes_forbidden_heading_once() {
        assert_eq!(
            guard_or_failure("Summary\n작업이 완료되었습니다."),
            "작업이 완료되었습니다."
        );
    }

    #[test]
    fn patch_verification_failure_contract_is_preserved() {
        let report = "패치 승인 실패\n- status: verification-failed-rolled-back\n- proposal id: proposal-1\n- path: src/lib.rs\n- approval token: accepted\n- original sha256: aaa\n- attempted sha256: bbb\n- actual source sha256: aaa\n- rollback record: rollback.json\n- rollback status: restored\n- verification command: cargo test\n- verification exit code: 1\n- verification stdout: none\n- verification stderr: failed\n- ledger event: event-1\n- boundary: patch verification과 rollback 결과를 실제 bytes/hash로 확인했으며 성공으로 보고하지 않습니다.";
        assert_eq!(guard_or_failure(report), report);
    }
}
