//! Binds model-authored claims to runtime-owned web source identities.

use std::collections::BTreeMap;

use crate::adapters::web_search::WebSourceEvidence;

mod presentation;
mod sanitize;

const WEB_ANSWER_FALLBACK: &str =
    "웹 검색은 완료했지만 로컬 모델이 요약을 완성하지 못했습니다. 아래 검증 가능한 출처를 확인하세요.";

pub(super) fn render_grounded_answer(
    generated: Option<String>,
    fallback: Option<String>,
    sources: &[WebSourceEvidence],
) -> String {
    let source_map = sources
        .iter()
        .map(|source| (source.source_id.as_str(), source))
        .collect::<BTreeMap<_, _>>();
    let answer = generated
        .and_then(|answer| sanitize::grounded_candidate(&answer, &source_map))
        .or_else(|| fallback.and_then(|answer| sanitize::grounded_candidate(&answer, &source_map)))
        .unwrap_or_else(|| WEB_ANSWER_FALLBACK.to_string());
    presentation::attach_verified_sources(&answer, sources)
}

#[cfg(test)]
#[path = "answer_binding/tests.rs"]
mod tests;
