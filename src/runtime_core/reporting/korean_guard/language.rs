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

const FOREIGN_LANGUAGE_NAMES_KO: &[&str] = &[
    "영어",
    "일본어",
    "중국어",
    "프랑스어",
    "독일어",
    "스페인어",
    "이탈리아어",
    "포르투갈어",
    "러시아어",
    "아랍어",
    "힌디어",
    "베트남어",
    "태국어",
    "인도네시아어",
    "네덜란드어",
    "폴란드어",
    "터키어",
    "우크라이나어",
];

const FOREIGN_LANGUAGE_NAMES_EN: &[&str] = &[
    "english",
    "japanese",
    "chinese",
    "french",
    "german",
    "spanish",
    "italian",
    "portuguese",
    "russian",
    "arabic",
    "hindi",
    "vietnamese",
    "thai",
    "indonesian",
    "dutch",
    "polish",
    "turkish",
    "ukrainian",
];

pub(super) fn allows_non_korean(prompt: &str) -> bool {
    let user_prompt = prompt
        .split_once("\n\n<attachment ")
        .map(|(prompt, _)| prompt)
        .unwrap_or(prompt)
        .to_lowercase();
    let foreign = latest_korean_directive(&user_prompt, FOREIGN_LANGUAGE_NAMES_KO)
        .into_iter()
        .chain(latest_english_directive(
            &user_prompt,
            FOREIGN_LANGUAGE_NAMES_EN,
        ))
        .chain(user_prompt.rfind("외국어로 번역"))
        .max();
    let korean = latest_korean_directive(&user_prompt, &["한국어", "한글"])
        .into_iter()
        .chain(latest_english_directive(&user_prompt, &["korean"]))
        .max();
    matches!((foreign, korean), (Some(_), None))
        || matches!((foreign, korean), (Some(foreign), Some(korean)) if foreign > korean)
}

fn latest_korean_directive(prompt: &str, languages: &[&str]) -> Option<usize> {
    languages
        .iter()
        .flat_map(|language| {
            KOREAN_OUTPUT_ACTIONS
                .iter()
                .map(move |action| format!("{language}{action}"))
        })
        .filter_map(|pattern| prompt.rfind(&pattern))
        .max()
}

fn latest_english_directive(prompt: &str, languages: &[&str]) -> Option<usize> {
    let direct = ENGLISH_OUTPUT_ACTIONS
        .iter()
        .flat_map(|action| {
            languages
                .iter()
                .map(move |language| format!("{action}{language}"))
        })
        .filter_map(|pattern| prompt.rfind(&pattern))
        .max();
    let translated = languages
        .iter()
        .filter_map(|language| latest_translation_target(prompt, language))
        .max();
    direct.into_iter().chain(translated).max()
}

fn latest_translation_target(prompt: &str, language: &str) -> Option<usize> {
    let target = format!("to {language}");
    prompt
        .match_indices(&target)
        .filter_map(|(target_index, _)| {
            prompt[..target_index]
                .rfind("translate")
                .filter(|translate_index| target_index.saturating_sub(*translate_index) <= 96)
                .map(|_| target_index)
        })
        .max()
}
