use crate::runtime_core::inference::backend::ResponseLanguage;
use crate::surfaces::tui::runtime_bridge::TuiVisionStatus;

mod model_query;

use model_query::ModelFactQuery;

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
    if is_local_access_status_request(request) && !has_agent_task_signal(request) {
        return Some(
            "가능합니다. rpotato는 현재 프로젝트 범위에서 파일 읽기, 디렉터리 목록, 코드 검색과 제한된 읽기 전용 명령을 직접 실행할 수 있습니다. 프로젝트 밖 접근과 변경 작업은 별도 정책·승인 경계를 따릅니다."
                .to_string(),
        );
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
    if !has_agent_task_signal(request)
        && ModelFactQuery::classify(request).is_current_identity_request()
    {
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

fn is_local_access_status_request(request: &str) -> bool {
    let lower = request.trim().to_ascii_lowercase();
    let mentions_local_access = [
        "로컬 파일",
        "파일시스템",
        "파일 시스템",
        "프로젝트 파일",
        "local file",
        "filesystem",
        "file system",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    let asks_capability = [
        "접근",
        "읽",
        "가능",
        "할 수",
        "access",
        "read",
        "available",
        "can you",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    mentions_local_access && asks_capability
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
