//! Structured contract between the local model and runtime-owned web presentation.

use std::collections::BTreeSet;

use crate::adapters::web_search::WebSourceEvidence;
use crate::app::inference_adapter::answer::GeneratedCandidate;
use crate::foundation::error::AppError;
use crate::foundation::serialization::{self, Value};

pub(super) const GROUNDED_ANSWER_JSON_SCHEMA: &str = r#"{"type":"object","properties":{"status":{"type":"string","enum":["supported","insufficient"]},"answer":{"type":"string"},"source_ids":{"type":"array","items":{"type":"string"},"maxItems":8}},"required":["status","answer","source_ids"],"additionalProperties":false}"#;

const MAX_ANSWER_CHARS: usize = 8 * 1024;
const MAX_SOURCE_IDS: usize = 8;

pub(super) fn finish(
    candidate: GeneratedCandidate,
    sources: &[WebSourceEvidence],
) -> Result<String, AppError> {
    let answer = parse(&candidate.visible, sources)?;
    crate::app::inference_adapter::answer::finish_candidate(GeneratedCandidate {
        response_language: candidate.response_language,
        visible: answer,
    })
}

fn parse(candidate: &str, sources: &[WebSourceEvidence]) -> Result<String, AppError> {
    let object = serialization::parse_object(
        candidate,
        &["status", "answer", "source_ids"],
        "grounded web answer",
    )?;
    let status = serialization::string(&object, "status", "grounded web answer")?;
    if !matches!(status.as_str(), "supported" | "insufficient") {
        return Err(invalid("status must be supported or insufficient"));
    }
    let answer = serialization::string(&object, "answer", "grounded web answer")?;
    let answer_length = answer.chars().count();
    if answer.trim().is_empty()
        || answer_length > MAX_ANSWER_CHARS
        || answer
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(invalid("answer is empty, oversized, or contains controls"));
    }
    let Some(Value::Array(values)) = object.get("source_ids") else {
        return Err(invalid("source_ids must be an array"));
    };
    if values.is_empty() || values.len() > MAX_SOURCE_IDS {
        return Err(invalid(
            "source_ids must contain between one and eight entries",
        ));
    }
    let available = sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut declared = BTreeSet::new();
    for value in values {
        let Value::String(source_id) = value else {
            return Err(invalid("source_ids entries must be strings"));
        };
        if !source_id.starts_with("source-")
            || !available.contains(source_id.as_str())
            || !declared.insert(source_id.as_str())
            || !answer.contains(&format!("[{source_id}]"))
        {
            return Err(invalid(
                "source_ids must be unique runtime source ids cited by answer",
            ));
        }
    }
    let cited = cited_source_ids(&answer);
    if cited != declared {
        return Err(invalid(
            "source_ids must exactly match runtime source ids cited by answer",
        ));
    }
    Ok(answer)
}

fn cited_source_ids(answer: &str) -> BTreeSet<&str> {
    let mut cited = BTreeSet::new();
    let mut remaining = answer;
    while let Some(start) = remaining.find("[source-") {
        let candidate = &remaining[start + 1..];
        let Some(end) = candidate.find(']') else {
            break;
        };
        cited.insert(&candidate[..end]);
        remaining = &candidate[end + 1..];
    }
    cited
}

fn invalid(reason: &str) -> AppError {
    AppError::blocked(format!("웹 근거 답변 계약을 만족하지 못했습니다: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::inference::backend::ResponseLanguage;

    fn source(id: &str) -> WebSourceEvidence {
        WebSourceEvidence {
            source_id: id.to_string(),
            title: "Primary source".to_string(),
            url: "https://example.com/source".to_string(),
        }
    }

    #[test]
    fn accepts_only_declared_runtime_sources_cited_in_the_answer() {
        let candidate = GeneratedCandidate {
            response_language: ResponseLanguage::KoreanDefault,
            visible: r#"{"status":"supported","answer":"확인된 답입니다. [source-primary]","source_ids":["source-primary"]}"#.to_string(),
        };

        let answer = finish(candidate, &[source("source-primary")]).unwrap();

        assert_eq!(answer, "확인된 답입니다. [source-primary]");
    }

    #[test]
    fn rejects_unknown_duplicate_or_uncited_source_ids() {
        for visible in [
            r#"{"status":"supported","answer":"답 [source-other]","source_ids":["source-other"]}"#,
            r#"{"status":"supported","answer":"답 [source-primary]","source_ids":["source-primary","source-primary"]}"#,
            r#"{"status":"supported","answer":"답","source_ids":["source-primary"]}"#,
            r#"{"status":"supported","answer":"답 [source-primary] [source-other]","source_ids":["source-primary"]}"#,
        ] {
            let candidate = GeneratedCandidate {
                response_language: ResponseLanguage::KoreanDefault,
                visible: visible.to_string(),
            };
            assert!(
                finish(candidate, &[source("source-primary")]).is_err(),
                "{visible}"
            );
        }
    }
}
