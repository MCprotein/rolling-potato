//! Korean-output policy facade for strict runtime reports and model responses.

mod classification;
mod language;
mod projection;
mod streaming;

pub use streaming::StreamingGuard;

use classification::classify_outside_text;
use projection::stricter_projection;

const FAILURE: &str =
    "응답 언어 검증에 실패했습니다. 출력이 한국어 기준을 만족하지 않아 결과를 표시하지 않았습니다.";

pub fn allows_non_korean(prompt: &str) -> bool {
    language::allows_non_korean(prompt)
}

pub fn guard_or_failure(text: &str) -> String {
    if let Some(projection) = safe_projection(text).filter(|projection| projection != text) {
        return projection;
    }
    guard_with_regeneration(text, || stricter_projection(text))
}

pub(crate) fn safe_projection(text: &str) -> Option<String> {
    let projection = stricter_projection(text);
    (!projection.trim().is_empty() && validate(&projection)).then_some(projection)
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
    let mut saw_language_neutral_content = false;
    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let line = classify_outside_text(trimmed);
        if line.forbidden {
            return false;
        }
        saw_hangul |= line.has_hangul;
        saw_language_neutral_content |= line.language_neutral;
    }
    saw_hangul || saw_language_neutral_content
}

#[cfg(test)]
mod tests;
