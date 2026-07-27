//! Surface-neutral restricted-browser action contracts.

mod interaction;

pub(crate) use interaction::{
    AdmittedBrowserAction, BrowserInteractionSession, ObservedTargetSeed,
};

#[cfg(test)]
const MAX_HANDLE_BYTES: usize = 96;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ElementHandle(String);

impl ElementHandle {
    #[cfg(test)]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        (!value.is_empty()
            && value.len() <= MAX_HANDLE_BYTES
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
        .then(|| Self(value.to_string()))
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn issued(revision: u64, index: usize) -> Self {
        Self(format!("element-{revision}-{index}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementRole {
    SearchBox,
    TextField,
    Button,
    Link,
    Checkbox,
    Radio,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedElement {
    pub(crate) handle: ElementHandle,
    pub(crate) role: ElementRole,
    pub(crate) name: String,
    pub(crate) disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserObservation {
    pub(crate) revision: u64,
    pub(crate) elements: Vec<ObservedElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrowserActionResult {
    Navigated { url: String },
    Observation(BrowserObservation),
    Clicked,
    Typed,
    KeyPressed,
    Scrolled,
    Extracted { text: String },
    CurrentUrl { url: String },
    Screenshot { png_base64: String },
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Closed browser-tool key contract; the search-form coordinator uses Enter.
pub(crate) enum BrowserKey {
    Enter,
    Escape,
    Tab,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Closed browser-tool scroll contract; B3 search-form does not scroll.
pub(crate) enum ScrollDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Adapter supports the closed action contract beyond the B3 search-form flow.
pub(crate) enum BrowserAction {
    Navigate {
        url: String,
    },
    Observe,
    Click {
        handle: ElementHandle,
    },
    Type {
        handle: ElementHandle,
        text: String,
    },
    Press {
        key: BrowserKey,
    },
    Scroll {
        direction: ScrollDirection,
        viewports: u8,
    },
    Extract {
        max_chars: usize,
    },
    CurrentUrl,
    Screenshot,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserActionLimit {
    Actions,
    Observations,
    Interactions,
    ExtractedText,
    Elapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserActionBlock {
    InvalidAction,
    StaleHandle,
    ForbiddenTarget,
    Closed,
    BudgetReached(BrowserActionLimit),
}

impl BrowserActionBlock {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::InvalidAction => "브라우저 action parameter가 허용 범위를 벗어났습니다.",
            Self::StaleHandle => "화면이 변경되어 이전 element handle이 만료되었습니다.",
            Self::ForbiddenTarget => {
                "로그인·결제·파일 전송 또는 민감한 browser control은 실행하지 않습니다."
            }
            Self::Closed => "제한 브라우저 session이 이미 종료되었습니다.",
            Self::BudgetReached(BrowserActionLimit::Actions) => {
                "브라우저 action 횟수 상한에 도달했습니다."
            }
            Self::BudgetReached(BrowserActionLimit::Observations) => {
                "브라우저 화면 관찰 횟수 상한에 도달했습니다."
            }
            Self::BudgetReached(BrowserActionLimit::Interactions) => {
                "브라우저 입력·클릭 횟수 상한에 도달했습니다."
            }
            Self::BudgetReached(BrowserActionLimit::ExtractedText) => {
                "브라우저 추출 text 상한에 도달했습니다."
            }
            Self::BudgetReached(BrowserActionLimit::Elapsed) => {
                "브라우저 작업 시간 상한에 도달했습니다."
            }
        }
    }
}

#[cfg(test)]
#[path = "browser/tests.rs"]
mod tests;
