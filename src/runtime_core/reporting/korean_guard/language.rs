pub(super) fn allows_non_korean(prompt: &str) -> bool {
    let user_prompt = prompt
        .split_once("\n\n<attachment ")
        .map(|(prompt, _)| prompt)
        .unwrap_or(prompt)
        .to_lowercase();
    const FOREIGN_PATTERNS: &[&str] = &[
        "영어로 번역",
        "영어로 답",
        "영어로 작성",
        "영어로 써",
        "영어로 해",
        "영어로 출력",
        "일본어로 번역",
        "일본어로 답",
        "일본어로 작성",
        "일본어로 해",
        "중국어로 번역",
        "중국어로 답",
        "중국어로 작성",
        "중국어로 해",
        "프랑스어로 번역",
        "독일어로 번역",
        "스페인어로 번역",
        "외국어로 번역",
    ];
    const ENGLISH_PATTERNS: &[&str] = &[
        "answer in english",
        "reply in english",
        "respond in english",
        "translate to english",
        "answer in japanese",
        "reply in japanese",
        "respond in japanese",
        "translate to japanese",
        "answer in chinese",
        "reply in chinese",
        "respond in chinese",
        "translate to chinese",
        "answer in french",
        "reply in french",
        "translate to french",
        "answer in german",
        "reply in german",
        "translate to german",
        "answer in spanish",
        "reply in spanish",
        "translate to spanish",
    ];
    const KOREAN_OUTPUT_PATTERNS: &[&str] = &[
        "한국어로 답",
        "한국어로 작성",
        "한국어로 번역",
        "한글로 답",
        "한글로 작성",
        "answer in korean",
        "reply in korean",
        "respond in korean",
        "translate to korean",
    ];
    let foreign = FOREIGN_PATTERNS
        .iter()
        .chain(ENGLISH_PATTERNS)
        .filter_map(|pattern| user_prompt.rfind(pattern))
        .max();
    let korean = KOREAN_OUTPUT_PATTERNS
        .iter()
        .filter_map(|pattern| user_prompt.rfind(pattern))
        .max();
    matches!((foreign, korean), (Some(_), None))
        || matches!((foreign, korean), (Some(foreign), Some(korean)) if foreign > korean)
}
