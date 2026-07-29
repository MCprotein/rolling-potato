//! Deterministic fallback when the local model cannot synthesize opened web evidence.
//!
//! The fallback ranks evidence by lexical overlap only. It does not contain question-specific
//! facts, product names, event names, or answer templates for individual domains.

use super::WebGroundingEvidence;

const PASSAGE_CHARS: usize = 320;

pub(super) fn render(user_request: &str, grounding: &[WebGroundingEvidence]) -> Option<String> {
    let primary = grounding
        .iter()
        .max_by_key(|evidence| evidence_score(user_request, evidence))?;
    let passage = grounding
        .iter()
        .flat_map(|evidence| {
            extract_passages(&evidence.excerpt)
                .into_iter()
                .map(move |passage| (evidence, passage))
        })
        .max_by_key(|(_, passage)| super::routing::overlap_score(user_request, passage));
    let Some((passage_source, passage)) = passage else {
        let title = bounded_chars(&decode_display_entities(primary.title.trim()));
        return Some(format!(
            "검색 근거에서 “{title}” 문서를 확인했지만 답변으로 요약할 원문 구간은 찾지 못했습니다. [{}]",
            primary.source_id
        ));
    };
    let title = bounded_chars(&decode_display_entities(passage_source.title.trim()));
    let passage = bounded_chars(passage.trim());
    if passage.is_empty() || passage == title {
        return Some(format!(
            "검색 근거에는 “{title}”라고 표기되어 있습니다. [{}]",
            passage_source.source_id
        ));
    }
    Some(format!(
        "검색 근거 “{title}”의 관련 원문에는 “{passage}”라고 적혀 있습니다. [{}]",
        passage_source.source_id
    ))
}

fn evidence_score(user_request: &str, evidence: &WebGroundingEvidence) -> usize {
    super::routing::overlap_score(user_request, &evidence.title) * 3
        + super::routing::overlap_score(user_request, &evidence.excerpt)
}

fn meaningful_chars(value: &str) -> usize {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
}

fn extract_passages(excerpt: &str) -> Vec<String> {
    excerpt
        .lines()
        .flat_map(|line| {
            let decoded = decode_display_entities(line.trim());
            sentence_passages(&decoded)
        })
        .collect()
}

fn sentence_passages(line: &str) -> Vec<String> {
    let mut passages = Vec::new();
    let mut start = 0;
    let indexed = line.char_indices().collect::<Vec<_>>();
    for (position, (index, character)) in indexed.iter().copied().enumerate() {
        if !matches!(character, '.' | '!' | '?' | '。') {
            continue;
        }
        let decimal_point = character == '.'
            && position > 0
            && position + 1 < indexed.len()
            && indexed[position - 1].1.is_ascii_digit()
            && indexed[position + 1].1.is_ascii_digit();
        if decimal_point {
            continue;
        }
        let end = index + character.len_utf8();
        push_meaningful_passage(&mut passages, &line[start..end]);
        start = end;
    }
    if start < line.len() {
        push_meaningful_passage(&mut passages, &line[start..]);
    }
    passages
}

fn push_meaningful_passage(passages: &mut Vec<String>, passage: &str) {
    let passage = passage.trim();
    if meaningful_chars(passage) >= 8 {
        passages.push(passage.to_string());
    }
}

fn decode_display_entities(value: &str) -> String {
    value
        .replace("&ldquo;", "“")
        .replace("&rdquo;", "”")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn bounded_chars(value: &str) -> String {
    let mut chars = value.chars();
    let mut bounded = chars.by_ref().take(PASSAGE_CHARS).collect::<String>();
    if chars.next().is_some() {
        bounded.push('…');
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_selects_evidence_without_domain_specific_branches() {
        let evidence = vec![
            WebGroundingEvidence {
                source_id: "source-unrelated".to_string(),
                title: "Unrelated page".to_string(),
                url: "https://example.com/unrelated".to_string(),
                excerpt: "다른 주제의 설명입니다.".to_string(),
            },
            WebGroundingEvidence {
                source_id: "source-primary".to_string(),
                title: "Alpha runtime release".to_string(),
                url: "https://example.com/release".to_string(),
                excerpt: "Alpha runtime version 2.0 was published today.".to_string(),
            },
        ];

        let answer = render("Alpha runtime 최신 release", &evidence).unwrap();

        assert!(answer.contains("Alpha runtime release"), "{answer}");
        assert!(answer.contains("version 2.0"), "{answer}");
        assert!(answer.contains("[source-primary]"), "{answer}");
    }

    #[test]
    fn fallback_keeps_title_passage_and_source_id_from_the_same_evidence() {
        let evidence = vec![
            WebGroundingEvidence {
                source_id: "source-title".to_string(),
                title: "Alpha runtime release performance".to_string(),
                url: "https://example.com/release".to_string(),
                excerpt: "이 문서는 일반적인 출시 안내를 제공합니다.".to_string(),
            },
            WebGroundingEvidence {
                source_id: "source-passage".to_string(),
                title: "Independent benchmark notes".to_string(),
                url: "https://example.com/benchmark".to_string(),
                excerpt: "Alpha runtime performance improved by 30 percent in the benchmark."
                    .to_string(),
            },
        ];

        let answer = render("Alpha runtime release performance", &evidence).unwrap();

        assert!(answer.contains("Independent benchmark notes"), "{answer}");
        assert!(answer.contains("improved by 30 percent"), "{answer}");
        assert!(answer.contains("[source-passage]"), "{answer}");
        assert!(
            !answer.contains("Alpha runtime release performance”"),
            "{answer}"
        );
        assert!(!answer.contains("[source-title]"), "{answer}");
    }
}
