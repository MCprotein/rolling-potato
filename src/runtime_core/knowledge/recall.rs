//! Dependency-free dialogue memory classification and query-driven recall.

use std::collections::BTreeSet;

use super::compaction::estimate_tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogueRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DialogueTurn {
    pub(crate) role: DialogueRole,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DialogueMemoryPlan {
    pub(crate) typed_user_memory: Vec<DialogueTurn>,
    pub(crate) recalled_history: Vec<DialogueTurn>,
    pub(crate) recent_history: Vec<DialogueTurn>,
}

pub(crate) fn plan_dialogue_memory(
    turns: &[DialogueTurn],
    query: &str,
    typed_budget_tokens: usize,
    recall_budget_tokens: usize,
    recent_budget_tokens: usize,
) -> DialogueMemoryPlan {
    let pairs = completed_pairs(turns);
    let recent_pair_count = pairs.len().min(8);
    let recent_start = pairs.len().saturating_sub(recent_pair_count);
    let recent_history =
        select_recent_pairs_within_budget(&pairs[recent_start..], recent_budget_tokens);

    let older = &pairs[..recent_start];
    let typed_indices = older
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, pair)| is_typed_user_memory(&pair[0].content))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let typed_user_memory =
        select_indexed_pairs_within_budget(older, &typed_indices, typed_budget_tokens);
    let typed_set = typed_indices.into_iter().collect::<BTreeSet<_>>();

    let query_features = lexical_features(query);
    let mut ranked = older
        .iter()
        .enumerate()
        .filter(|(index, _)| !typed_set.contains(index))
        .map(|(index, pair)| {
            let content = format!("{}\n{}", pair[0].content, pair[1].content);
            let overlap = lexical_features(&content)
                .intersection(&query_features)
                .count();
            let recency = index.saturating_mul(2) / older.len().max(1);
            (index, overlap.saturating_mul(16).saturating_add(recency))
        })
        .filter(|(_, score)| *score > 0)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));
    let recalled_indices = ranked
        .into_iter()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let recalled_history =
        select_indexed_pairs_within_budget(older, &recalled_indices, recall_budget_tokens);

    DialogueMemoryPlan {
        typed_user_memory,
        recalled_history,
        recent_history,
    }
}

fn completed_pairs(turns: &[DialogueTurn]) -> Vec<&[DialogueTurn]> {
    turns
        .chunks_exact(2)
        .filter(|pair| {
            pair[0].role == DialogueRole::User && pair[1].role == DialogueRole::Assistant
        })
        .collect()
}

fn select_indexed_pairs_within_budget(
    pairs: &[&[DialogueTurn]],
    ranked_indices: &[usize],
    budget_tokens: usize,
) -> Vec<DialogueTurn> {
    let mut selected = Vec::new();
    let mut used = 0usize;
    for index in ranked_indices {
        let pair = pairs[*index];
        let cost = pair_token_cost(pair);
        if used.saturating_add(cost) > budget_tokens {
            continue;
        }
        used += cost;
        selected.push((*index, pair));
    }
    selected.sort_by_key(|(index, _)| *index);
    selected
        .into_iter()
        .flat_map(|(_, pair)| pair.iter().cloned())
        .collect()
}

fn select_recent_pairs_within_budget(
    pairs: &[&[DialogueTurn]],
    budget_tokens: usize,
) -> Vec<DialogueTurn> {
    let mut selected = Vec::new();
    let mut used = 0usize;
    for pair in pairs.iter().rev() {
        let cost = pair_token_cost(pair);
        if used.saturating_add(cost) > budget_tokens {
            continue;
        }
        used += cost;
        selected.push(*pair);
    }
    selected.reverse();
    selected
        .into_iter()
        .flat_map(|pair| pair.iter().cloned())
        .collect()
}

fn pair_token_cost(pair: &[DialogueTurn]) -> usize {
    pair.iter()
        .map(|turn| estimate_tokens(&turn.content).saturating_add(8))
        .sum()
}

fn is_typed_user_memory(content: &str) -> bool {
    let lower = content.to_lowercase();
    [
        "내 이름",
        "제 이름",
        "나는 ",
        "저는 ",
        "내가 ",
        "기억해",
        "앞으로",
        "선호",
        "좋아",
        "싫어",
        "원칙",
        "반드시",
        "하지 마",
        "하지마",
        "말고",
        "아니 ",
        "my name",
        "i am ",
        "i prefer",
        "remember",
        "always ",
        "never ",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

fn lexical_features(text: &str) -> BTreeSet<String> {
    let normalized = text.to_lowercase();
    let mut features = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    let compact = normalized
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<Vec<_>>();
    for window in compact.windows(2) {
        features.insert(window.iter().collect());
    }
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(user: &str, assistant: &str) -> [DialogueTurn; 2] {
        [
            DialogueTurn {
                role: DialogueRole::User,
                content: user.to_string(),
            },
            DialogueTurn {
                role: DialogueRole::Assistant,
                content: assistant.to_string(),
            },
        ]
    }

    #[test]
    fn typed_memory_and_query_recall_preserve_complete_pairs_and_chronology() {
        let mut turns = Vec::new();
        turns.extend(pair("내 이름은 감자야", "기억할게."));
        turns.extend(pair(
            "Rust ownership을 설명해줘",
            "소유권은 자원 관리 규칙입니다.",
        ));
        for index in 0..9 {
            turns.extend(pair(
                &format!("최근 질문 {index}"),
                &format!("최근 답변 {index}"),
            ));
        }

        let plan = plan_dialogue_memory(&turns, "내 이름 기억해?", 512, 512, 512);

        assert_eq!(plan.typed_user_memory.len() % 2, 0);
        assert!(plan
            .typed_user_memory
            .iter()
            .any(|turn| turn.content.contains("감자")));
        assert_eq!(plan.recalled_history.len() % 2, 0);
        assert_eq!(plan.recent_history.len() % 2, 0);
        assert!(plan
            .recent_history
            .first()
            .is_some_and(|turn| turn.content.contains("최근 질문")));
    }
}
