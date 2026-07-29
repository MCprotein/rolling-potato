use super::features::{GroundingQueryKind, GroundingSignals};

const MAX_QUERY_CHARS: usize = 512;

pub(super) fn strengthen(query: &str, signals: GroundingSignals) -> String {
    let query = query.trim();
    let query_lower = query.to_ascii_lowercase();
    let hints = match signals.query_kind() {
        GroundingQueryKind::General => &[][..],
        GroundingQueryKind::CurrentFact => &["공식", "official"][..],
        GroundingQueryKind::Outcome => &["공식", "official", "result"][..],
        GroundingQueryKind::Comparison => &["공식", "benchmark", "methodology"][..],
    };
    let additions = hints
        .iter()
        .copied()
        .filter(|term| !query_lower.contains(&term.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return query.chars().take(MAX_QUERY_CHARS).collect();
    }

    let suffix = format!(" {}", additions.join(" "));
    let keep = MAX_QUERY_CHARS.saturating_sub(suffix.chars().count());
    let mut strengthened = query.chars().take(keep).collect::<String>();
    strengthened.push_str(&suffix);
    strengthened
}
