//! First-run model list and confirmation presentation.

use crate::runtime_core::terminal::TerminalChoice;

use super::TuiModelOption;

pub(super) fn render_setup_screen(options: &[TuiModelOption], color: bool) -> String {
    let mut output = String::new();
    output.push_str(&paint("rpotato 첫 실행 설정\n", "\u{001b}[1;36m", color));
    output.push_str("backend와 GGUF 경로는 자동으로 관리됩니다. 사용할 모델만 선택하세요.\n\n");
    for (index, option) in options.iter().enumerate() {
        let recommendation = if option.recommended { " [권장]" } else { "" };
        output.push_str(&format!(
            "{}. {}{}\n   id {} | {} | {} | context {} | RAM {} | {}\n   {}\n",
            index + 1,
            option.display_name,
            recommendation,
            option.id,
            option.quantization,
            option.model_artifact_label(),
            option
                .context_length
                .map(compact_tokens)
                .unwrap_or_else(|| "미확정".to_string()),
            option.ram,
            option.license,
            option.note
        ));
    }
    output.push('\n');
    output
}

pub(super) fn model_choices(options: &[TuiModelOption]) -> Vec<TerminalChoice> {
    let mut choices = options
        .iter()
        .map(|option| TerminalChoice {
            value: option.id.clone(),
            label: option.display_name.clone(),
            description: format!(
                "{} · {} · context {} · RAM {} · {}",
                option.quantization,
                option.model_artifact_label(),
                option
                    .context_length
                    .map(compact_tokens)
                    .unwrap_or_else(|| "미확정".to_string()),
                option.ram,
                option.license
            ),
            current: option.current,
            recommended: option.recommended,
        })
        .collect::<Vec<_>>();
    choices.push(TerminalChoice {
        value: "skip".to_string(),
        label: "나중에 설정".to_string(),
        description: "다운로드하지 않고 TUI를 시작합니다.".to_string(),
        current: false,
        recommended: false,
    });
    choices
}

pub(super) fn confirmation_choices(selected: &TuiModelOption) -> [TerminalChoice; 2] {
    [
        TerminalChoice {
            value: "cancel".to_string(),
            label: "취소".to_string(),
            description: "다운로드하거나 backend를 변경하지 않습니다.".to_string(),
            current: false,
            recommended: false,
        },
        TerminalChoice {
            value: "install".to_string(),
            label: if selected.model_cached {
                "기존 모델로 시작".to_string()
            } else {
                "설치하고 시작".to_string()
            },
            description: format!(
                "{} · {} · {}",
                selected.display_name,
                selected.model_artifact_label(),
                selected.license
            ),
            current: false,
            recommended: true,
        },
    ]
}

fn compact_tokens(tokens: u32) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}

fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{value}\u{001b}[0m")
    } else {
        value.to_string()
    }
}
