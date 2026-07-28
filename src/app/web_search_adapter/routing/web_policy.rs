pub(crate) fn web_disabled(request: &str) -> bool {
    let request = request.trim();
    let lower = request.to_ascii_lowercase();
    has_no_web_directive(&lower)
        || [
            "검색하지마",
            "검색하지 마",
            "검색하지 말",
            "검색 금지",
            "인터넷 쓰지마",
            "인터넷 쓰지 말",
            "인터넷 사용하지",
            "인터넷 없이",
            "웹 사용하지",
            "웹 없이",
            "외부 검색하지",
            "외부 네트워크에 연결하지",
            "외부 네트워크 연결하지",
            "네트워크에 연결하지",
            "네트워크 사용하지",
            "외부 연결하지",
            "오프라인",
        ]
        .iter()
        .any(|signal| request.contains(signal))
        || [
            "do not browse",
            "don't browse",
            "do not search",
            "don't search",
            "do not use the internet",
            "do not access the network",
            "don't access the network",
            "do not connect to the network",
            "don't connect to the network",
            "do not make network requests",
            "don't make network requests",
            "without browsing",
            "without internet",
            "without network access",
            "no web",
            "no browsing",
            "no network access",
            "offline",
        ]
        .iter()
        .any(|signal| contains_ascii_phrase(&lower, signal))
}

fn has_no_web_directive(request: &str) -> bool {
    request.strip_prefix("--no-web").is_some_and(|remaining| {
        remaining.is_empty() || remaining.chars().next().is_some_and(char::is_whitespace)
    })
}

fn contains_ascii_phrase(text: &str, phrase: &str) -> bool {
    let words = ascii_words(text);
    let phrase = ascii_words(phrase);
    !phrase.is_empty() && words.windows(phrase.len()).any(|window| window == phrase)
}

fn ascii_words(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}
