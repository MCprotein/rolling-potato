use super::super::render::{
    push_footer, push_header, push_kv, push_literal_block, push_rule, push_section, push_wrapped,
    terminal_width,
};
use super::super::runtime_bridge::TuiReadPage;

pub(crate) fn canonical_page_report(page: TuiReadPage) -> String {
    let width = terminal_width();
    let literal_content = page.title == "diff";
    let mut lines = Vec::new();
    push_header(
        &mut lines,
        width,
        &format!("rpotato TUI beta - {}", page.title),
    );
    push_kv(&mut lines, width, "page", &(page.page + 1).to_string());
    push_kv(&mut lines, width, "freshness", page.freshness.as_str());
    push_kv(
        &mut lines,
        width,
        "continuation",
        page.continuation.as_str(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "canonical authority");
    push_kv(
        &mut lines,
        width,
        "current",
        &authority_pair(
            page.authority.current_revision,
            page.authority.current_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "workflow",
        &authority_pair(
            page.authority.workflow_revision,
            page.authority.workflow_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "ledger",
        &authority_pair(
            page.authority.ledger_sequence,
            page.authority.ledger_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "content hash",
        page.authority
            .content_hash
            .as_deref()
            .unwrap_or("unavailable"),
    );
    push_kv(
        &mut lines,
        width,
        "validated at ms",
        &page
            .authority
            .validated_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "content");
    if page.lines.is_empty() {
        push_wrapped(&mut lines, width, "No canonical records are available.");
    } else {
        for (index, line) in page.lines.iter().enumerate() {
            if literal_content && index > 0 {
                push_literal_block(&mut lines, width, line);
            } else {
                push_wrapped(&mut lines, width, line);
            }
        }
    }
    push_footer(&mut lines, width);
    lines.join("\n")
}

fn authority_pair(revision: Option<u64>, hash: Option<&str>) -> String {
    match (revision, hash) {
        (Some(revision), Some(hash)) => format!("revision={revision} hash={hash}"),
        _ => "unavailable".to_string(),
    }
}
