use super::ascii_words;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModelQueryTarget {
    CurrentRuntime,
    Contextual,
    Unspecified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModelQueryIntent {
    Identity,
    Recommendation,
    Comparison,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ModelFactQuery {
    target: ModelQueryTarget,
    intent: ModelQueryIntent,
}

impl ModelFactQuery {
    pub(super) fn classify(request: &str) -> Self {
        let lower = request.trim().to_ascii_lowercase();
        let compact = lower
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let direct = compact.trim_matches(|character: char| {
            character.is_ascii_punctuation() || matches!(character, '？' | '。' | '！' | '…' | '·')
        });
        let words = ascii_words(&lower);
        let mentions_model =
            direct.contains("모델") || words.iter().any(|word| matches!(*word, "model" | "models"));
        let current_marker = ["현재", "지금", "사용중", "설정된"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "current" | "using" | "now"));
        let runtime_target = ["너", "넌", "너는"]
            .iter()
            .any(|signal| direct.starts_with(signal))
            || words
                .windows(2)
                .any(|window| matches!(window, ["are", "you"] | ["you", "using"]));
        let contextual = ["전에", "아까", "기억", "말했", "선호", "내가"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "remember" | "previously" | "earlier"));
        let recommendation = ["추천", "설치", "선택"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "recommend" | "install" | "choose"));
        let comparison = ["비교", "성능", "대결", "vs"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "compare" | "comparison" | "versus" | "performance"));
        let identity_question = ["무슨", "어떤", "뭐", "뭔", "이름", "명칭"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "what" | "which" | "name"));
        let runtime_usage = ["쓰", "사용", "구동", "실행", "설정"]
            .iter()
            .any(|signal| direct.contains(signal))
            || words
                .iter()
                .any(|word| matches!(*word, "using" | "running"));
        let identifier_challenge =
            runtime_target && current_marker && has_identifier_assertion(&lower, direct);

        let target = if contextual {
            ModelQueryTarget::Contextual
        } else if current_marker || runtime_target || (mentions_model && identity_question) {
            ModelQueryTarget::CurrentRuntime
        } else {
            ModelQueryTarget::Unspecified
        };
        let intent = if recommendation {
            ModelQueryIntent::Recommendation
        } else if comparison {
            ModelQueryIntent::Comparison
        } else if (mentions_model && (identity_question || current_marker || runtime_usage))
            || identifier_challenge
        {
            ModelQueryIntent::Identity
        } else {
            ModelQueryIntent::Other
        };

        Self { target, intent }
    }

    pub(super) fn is_current_identity_request(self) -> bool {
        self.target == ModelQueryTarget::CurrentRuntime && self.intent == ModelQueryIntent::Identity
    }
}

fn has_identifier_assertion(lower: &str, compact: &str) -> bool {
    let asserts_identity = ["잖아", "아니야", "맞지", "맞냐"]
        .iter()
        .any(|ending| compact.contains(ending));
    if !asserts_identity {
        return false;
    }

    ascii_words(lower).iter().any(|word| {
        word.len() >= 2
            && word
                .chars()
                .any(|character| character.is_ascii_alphabetic())
            && !matches!(
                *word,
                "you" | "are" | "using" | "current" | "model" | "right" | "now"
            )
    })
}
