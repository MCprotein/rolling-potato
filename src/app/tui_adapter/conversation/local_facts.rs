use crate::runtime_core::inference::backend::ResponseLanguage;
use crate::surfaces::tui::runtime_bridge::TuiVisionStatus;

pub(in crate::app::tui_adapter) fn is_conversational_request(request: &str) -> bool {
    let trimmed = request.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= 2_000 && !has_agent_task_signal(trimmed)
}

pub(in crate::app::tui_adapter) fn local_reply(
    request: &str,
    model: Option<&str>,
    vision: TuiVisionStatus,
) -> Option<String> {
    if ResponseLanguage::from_user_request(request).allows_non_korean() {
        return None;
    }
    if is_vision_status_request(request) && !has_agent_task_signal(request) {
        let model = model.unwrap_or("현재 모델");
        return Some(match vision {
            TuiVisionStatus::Ready => format!(
                "{model}의 이미지 입력이 준비되어 있습니다. 첨부한 이미지를 바로 분석할 수 있습니다."
            ),
            TuiVisionStatus::OnDemand => format!(
                "{model}은 이미지 입력을 지원합니다. `vision on-demand`는 미지원이 아니라 projector를 아직 backend에 올리지 않았다는 뜻입니다. 이미지를 첨부하면 필요한 projector를 검증·준비하고 비전 backend로 자동 전환하며, 준비된 cache는 다음 요청부터 재사용합니다."
            ),
            TuiVisionStatus::Unsupported => format!(
                "{model}에는 검증된 vision projector가 없어 이미지 입력을 지원하지 않습니다. rpotato 자체의 비전 기능이 꺼진 것은 아닙니다."
            ),
            TuiVisionStatus::Unavailable => {
                "현재 모델의 비전 상태를 확인할 수 없습니다. /model에서 모델을 선택하세요."
                    .to_string()
            }
        });
    }
    if ModelFactQuery::classify(request).is_current_identity_request() {
        return Some(
            match model.map(str::trim).filter(|value| !value.is_empty()) {
                Some(model) => format!("현재 사용 중인 모델은 {model}입니다."),
                None => {
                    "현재 선택된 모델이 없습니다. /model로 모델을 선택할 수 있습니다.".to_string()
                }
            },
        );
    }
    is_agent_identity_request(request)
        .then(|| "저는 로컬에서 실행되는 범용 AI·코딩 에이전트 rpotato입니다.".to_string())
}

fn is_vision_status_request(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    let mentions_vision = [
        "비전",
        "이미지",
        "멀티모달",
        "vision",
        "image",
        "multimodal",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    let asks_status = [
        "왜",
        "지원",
        "되",
        "가능",
        "상태",
        "text-only",
        "on-demand",
        "ready",
        "why",
        "support",
        "available",
        "status",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    mentions_vision && asks_status
}

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
    Task,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModelFactQuery {
    target: ModelQueryTarget,
    intent: ModelQueryIntent,
}

impl ModelFactQuery {
    fn classify(request: &str) -> Self {
        if has_agent_task_signal(request) {
            return Self {
                target: ModelQueryTarget::Unspecified,
                intent: ModelQueryIntent::Task,
            };
        }

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

    fn is_current_identity_request(self) -> bool {
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

fn is_agent_identity_request(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    let compact = lower
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let direct = compact.trim_matches(|character: char| {
        character.is_ascii_punctuation() || matches!(character, '？' | '。' | '！' | '…' | '·')
    });
    matches!(
        direct,
        "넌누구"
            | "넌누구야"
            | "넌누구니"
            | "너는누구"
            | "너는누구야"
            | "너는누구니"
            | "너누구"
            | "너누구야"
            | "너누구니"
            | "네정체가뭐야"
            | "너정체가뭐야"
            | "네이름이뭐야"
            | "네이름뭐야"
            | "네이름이뭔데"
            | "너이름이뭐야"
            | "너이름뭐야"
            | "너이름이뭔데"
    ) || matches!(
        lower.trim_matches(
            |character: char| character.is_ascii_punctuation() || character.is_whitespace()
        ),
        "who are you" | "what is your name"
    )
}

fn has_agent_task_signal(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    let words = ascii_words(&lower);
    let english_mutation = ["fix", "change", "edit", "implement", "refactor"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_failure = ["error", "crash", "crashes", "startup"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_local_scope = ["file", "code", "repo", "repository", "codebase", "project"]
        .iter()
        .any(|signal| words.contains(signal));
    let english_action = is_english_action_request(&words);
    let korean_action = ["고쳐", "수정", "변경", "구현", "리팩터", "테스트", "리뷰"]
        .iter()
        .any(|signal| request.contains(signal));
    let korean_local_scope = [
        "파일",
        "코드",
        "저장소",
        "프로젝트",
        "디렉터리",
        "경로",
        "소스",
    ]
    .iter()
    .any(|signal| request.contains(signal));
    let korean_local_action = [
        "알려", "보여", "열어", "확인", "구조", "내용", "어디", "분석", "찾아",
    ]
    .iter()
    .any(|signal| request.contains(signal));
    let korean_failure_analysis = ["오류", "에러", "실패", "크래시"]
        .iter()
        .any(|signal| request.contains(signal))
        && ["분석", "원인", "왜"]
            .iter()
            .any(|signal| request.contains(signal));

    english_mutation
        || english_failure
        || (english_local_scope && english_action)
        || korean_action
        || korean_failure_analysis
        || (korean_local_scope && korean_local_action)
}

fn ascii_words(text: &str) -> Vec<&str> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_english_action_request(words: &[&str]) -> bool {
    const ACTIONS: &[&str] = &[
        "test", "review", "analyze", "search", "show", "open", "read", "find", "explain",
    ];
    words.first().is_some_and(|word| ACTIONS.contains(word))
        || words
            .windows(2)
            .any(|window| window[0] == "please" && ACTIONS.contains(&window[1]))
        || words.windows(3).any(|window| {
            matches!(window[0], "can" | "could" | "would")
                && window[1] == "you"
                && ACTIONS.contains(&window[2])
        })
}
