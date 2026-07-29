use std::collections::BTreeSet;

use crate::adapters::web_search::WebSourceEvidence;

pub(super) fn attach_verified_sources(answer: &str, sources: &[WebSourceEvidence]) -> String {
    let mut rendered = String::new();
    let mut attached = BTreeSet::new();
    for paragraph in answer
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
    {
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(paragraph);
        for source in sources
            .iter()
            .filter(|source| paragraph.contains(&format!("[{}]", source.source_id)))
        {
            rendered.push_str(&format!(
                "\n근거 · [{}] {} — {}",
                source.source_id, source.title, source.url
            ));
            attached.insert(source.source_id.as_str());
        }
    }
    if attached.is_empty() {
        rendered.push_str("\n\n검증된 출처");
        for source in sources {
            rendered.push_str(&format!(
                "\n- [{}] {} — {}",
                source.source_id, source.title, source.url
            ));
        }
    }
    rendered
}
