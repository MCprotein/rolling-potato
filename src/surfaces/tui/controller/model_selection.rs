use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{TerminalChoice, TerminalFault, TerminalIo};

use super::super::runtime_bridge::TuiModelOption;
use super::terminal_flow::terminal_fault_error;
use super::TuiRuntimePort;

pub(super) fn model_options_notice(options: &[TuiModelOption]) -> String {
    let mut lines = vec!["사용 가능한 모델".to_string()];
    for option in options {
        let recommendation = if option.recommended { " | 권장" } else { "" };
        let current = if option.current { " | 현재" } else { "" };
        lines.push(format!(
            "- {} | {} | {} | context {} | RAM {} | {}{}{}\n  근거: {}",
            option.id,
            option.quantization,
            bytes_label(option.download_bytes),
            option
                .context_length
                .map(compact_tokens)
                .unwrap_or_else(|| "미확정".to_string()),
            option.ram,
            option.license,
            current,
            recommendation,
            option.note,
        ));
    }
    lines.push("변경: /model을 열어 ↑↓와 Enter로 선택하세요.".to_string());
    lines.join("\n")
}

pub(super) fn choose_model(
    terminal: &mut impl TerminalIo,
    options: &[TuiModelOption],
) -> Result<Option<String>, AppError> {
    let choices = options
        .iter()
        .map(|option| TerminalChoice {
            value: option.id.clone(),
            label: option.display_name.clone(),
            description: format!(
                "id {} · {} · {} · context {} · RAM {} · {} · {}",
                option.id,
                option.quantization,
                bytes_label(option.download_bytes),
                option
                    .context_length
                    .map(compact_tokens)
                    .unwrap_or_else(|| "미확정".to_string()),
                option.ram,
                option.license,
                option.note
            ),
            current: option.current,
            recommended: option.recommended,
        })
        .collect::<Vec<_>>();
    terminal
        .choose("모델 선택", &choices)
        .map_err(terminal_fault_error)
}

pub(super) fn apply_model_choice(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    selected: &TuiModelOption,
) -> Result<String, AppError> {
    if selected.current {
        return Ok(format!(
            "이미 사용 중인 모델입니다: {}",
            selected.display_name
        ));
    }
    let confirmation = [
        TerminalChoice {
            value: "apply".to_string(),
            label: "다운로드하고 적용".to_string(),
            description: format!(
                "{} · {} · SHA-256 검증 후 기본 모델로 전환",
                selected.display_name,
                bytes_label(selected.download_bytes)
            ),
            current: true,
            recommended: true,
        },
        TerminalChoice {
            value: "cancel".to_string(),
            label: "취소".to_string(),
            description: "현재 모델과 backend를 변경하지 않습니다.".to_string(),
            current: false,
            recommended: false,
        },
    ];
    if terminal
        .choose("모델 변경 확인", &confirmation)
        .map_err(terminal_fault_error)?
        .as_deref()
        != Some("apply")
    {
        return Ok("모델 변경을 취소했습니다.".to_string());
    }
    terminal
        .write_frame("backend 준비 → 모델 다운로드/SHA-256 검증 → 기본 모델 적용 중...\n")
        .map_err(|_| terminal_fault_error(TerminalFault::FrameWrite))?;
    Ok(model_setup_notice(runtime.setup_model(&selected.id)))
}

fn model_setup_notice(result: Result<String, AppError>) -> String {
    match result {
        Ok(report) => report,
        Err(error) => format!(
            "모델 변경 실패\n- 이유: {}\n- TUI는 계속 실행됩니다. /model로 다시 선택하세요.",
            error.message
        ),
    }
}

fn bytes_label(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.1} GiB", bytes as f64 / GIB)
}

fn compact_tokens(tokens: u32) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::model_setup_notice;
    use crate::foundation::error::AppError;

    #[test]
    fn recoverable_model_setup_error_becomes_a_notice_instead_of_exiting() {
        let notice = model_setup_notice(Err(AppError::runtime("download failed")));

        assert!(notice.contains("모델 변경 실패"));
        assert!(notice.contains("download failed"));
        assert!(notice.contains("TUI는 계속 실행됩니다"));
    }
}
