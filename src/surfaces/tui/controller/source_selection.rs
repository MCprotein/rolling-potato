use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{TerminalChoice, TerminalIo};

use super::super::runtime_bridge::TuiWebSourceOption;
use super::terminal_flow::{terminal_fault_error, write_pending_conversation_frame};
use super::TuiRuntimePort;
use crate::surfaces::tui::view_model::InteractiveState;

pub(super) fn select_source(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    width: u16,
    height: u16,
) -> Result<(), AppError> {
    let options = runtime.web_source_options();
    if options.is_empty() {
        state.notice = "열린 웹 출처가 없습니다. 먼저 검색 결과의 URL을 열어보세요.".to_string();
        return Ok(());
    }
    let Some(source_id) = choose_source(terminal, &options)? else {
        state.notice = "웹 출처 선택을 취소했습니다.".to_string();
        return Ok(());
    };
    let selected = options
        .iter()
        .find(|option| option.source_id == source_id)
        .expect("terminal source choice must originate from source options");
    state.notice = if selected.opened {
        "웹 출처 전환 중 · 현재 문서를 변경하고 있습니다…".to_string()
    } else {
        "웹 조사 · 페이지 여는 중\n문서 읽기 ● → 증거 구성 ○ → 답변 ○".to_string()
    };
    write_pending_conversation_frame(terminal, runtime, state, width, height)?;
    state.notice = runtime
        .select_web_source(&source_id)
        .unwrap_or_else(|error| error.message);
    Ok(())
}

fn choose_source(
    terminal: &mut impl TerminalIo,
    options: &[TuiWebSourceOption],
) -> Result<Option<String>, AppError> {
    let choices = options
        .iter()
        .map(|option| TerminalChoice {
            value: option.source_id.clone(),
            label: bounded(&option.title, 40),
            description: bounded(
                &format!(
                    "{} · {}",
                    if option.opened { "열림" } else { "열기" },
                    option.url
                ),
                88,
            ),
            current: option.current,
            recommended: false,
        })
        .collect::<Vec<_>>();
    terminal
        .choose("웹 출처 선택", &choices)
        .map_err(terminal_fault_error)
}

fn bounded(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_picker_labels_are_bounded_for_narrow_terminals() {
        assert_eq!(bounded(&"가".repeat(60), 8), "가가가가가가가…");
        assert_eq!(bounded("short", 8), "short");
    }
}
