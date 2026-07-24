const KOREAN_OUTPUT_ACTIONS: &[&str] = &[
    "로 번역",
    "로 답",
    "로 작성",
    "로 써",
    "로 해",
    "로 출력",
    "로 요약",
    "로 말",
];

const ENGLISH_OUTPUT_ACTIONS: &[&str] = &[
    "answer in ",
    "reply in ",
    "respond in ",
    "write in ",
    "translate to ",
    "summarize in ",
];

const NON_LANGUAGE_KOREAN_TARGETS: &[&str] = &["구어", "단어", "문어", "언어", "용어", "자연어"];

const NON_LANGUAGE_ENGLISH_TARGETS: &[&str] = &[
    "a",
    "an",
    "brief",
    "bullet",
    "bullets",
    "code",
    "csv",
    "detail",
    "full",
    "javascript",
    "json",
    "list",
    "markdown",
    "one",
    "paragraph",
    "paragraphs",
    "plain",
    "points",
    "prose",
    "python",
    "rust",
    "sentence",
    "sentences",
    "short",
    "steps",
    "table",
    "this",
    "typescript",
    "xml",
    "yaml",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestedLanguage {
    Korean,
    Other,
}

pub(super) fn allows_non_korean(prompt: &str) -> bool {
    let user_prompt = prompt
        .split_once("\n\n<attachment ")
        .map(|(prompt, _)| prompt)
        .unwrap_or(prompt)
        .to_lowercase();
    latest_korean_directive(&user_prompt)
        .into_iter()
        .chain(latest_english_directive(&user_prompt))
        .max_by_key(|(index, _)| *index)
        .is_some_and(|(_, language)| language == RequestedLanguage::Other)
}

fn latest_korean_directive(prompt: &str) -> Option<(usize, RequestedLanguage)> {
    KOREAN_OUTPUT_ACTIONS
        .iter()
        .flat_map(|action| prompt.match_indices(action))
        .filter_map(|(action_index, _)| {
            let (language_index, language) = preceding_word(prompt, action_index)?;
            let requested = if matches!(language, "한국어" | "한글") {
                RequestedLanguage::Korean
            } else if NON_LANGUAGE_KOREAN_TARGETS.contains(&language) {
                return None;
            } else if language == "외국어" || language.ends_with('어') {
                RequestedLanguage::Other
            } else {
                return None;
            };
            Some((language_index, requested))
        })
        .max_by_key(|(index, _)| *index)
}

fn latest_english_directive(prompt: &str) -> Option<(usize, RequestedLanguage)> {
    let direct = ENGLISH_OUTPUT_ACTIONS
        .iter()
        .flat_map(|action| {
            prompt
                .match_indices(action)
                .map(move |match_| (match_, *action))
        })
        .filter_map(|((action_index, _), action)| {
            let target_index = action_index + action.len();
            requested_english_target(prompt, target_index).map(|language| (target_index, language))
        })
        .max_by_key(|(index, _)| *index);
    direct
        .into_iter()
        .chain(latest_translation_target(prompt))
        .max_by_key(|(index, _)| *index)
}

fn latest_translation_target(prompt: &str) -> Option<(usize, RequestedLanguage)> {
    prompt
        .match_indices(" to ")
        .filter_map(|(separator_index, separator)| {
            let target_index = separator_index + separator.len();
            prompt[..target_index]
                .rfind("translate")
                .filter(|translate_index| separator_index.saturating_sub(*translate_index) <= 96)?;
            requested_english_target(prompt, target_index).map(|language| (target_index, language))
        })
        .max_by_key(|(index, _)| *index)
}

fn requested_english_target(prompt: &str, start: usize) -> Option<RequestedLanguage> {
    let target = prompt[start..]
        .trim_start()
        .split(|character: char| !character.is_ascii_alphabetic() && character != '-')
        .next()?;
    if target.is_empty() || NON_LANGUAGE_ENGLISH_TARGETS.contains(&target) {
        return None;
    }
    Some(if target == "korean" {
        RequestedLanguage::Korean
    } else {
        RequestedLanguage::Other
    })
}

fn preceding_word(prompt: &str, end: usize) -> Option<(usize, &str)> {
    let start = prompt[..end]
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_alphabetic() || ('가'..='힣').contains(character))
        .last()
        .map(|(index, _)| index)?;
    Some((start, &prompt[start..end]))
}
