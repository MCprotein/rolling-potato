use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{TerminalChoice, TerminalFault, TerminalIo};

use super::runtime_bridge::TuiModelOption;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedTuiModel {
    pub(crate) id: String,
    pub(crate) artifact_path: String,
    pub(crate) context_tokens: u32,
    pub(crate) vision_ready: bool,
}

pub(crate) trait TuiSetupPort {
    fn startup_update_notice(&mut self) -> Option<String>;
    fn model_options(&mut self) -> Vec<TuiModelOption>;
    fn ensure_backend(&mut self) -> Result<String, AppError>;
    fn prepare_model(&mut self, id: &str) -> Result<PreparedTuiModel, AppError>;
    fn start_model(&mut self, model: &PreparedTuiModel) -> Result<String, AppError>;
}

pub(crate) fn run_setup(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiSetupPort,
) -> Result<(), AppError> {
    terminal.validate_configuration().map_err(terminal_error)?;
    let options = runtime.model_options();
    if options.is_empty() {
        return Err(AppError::blocked(
            "초기 설정 차단\n- 이유: source-backed model 선택지가 없습니다.",
        ));
    }
    terminal
        .write_frame(&render_setup_screen(&options, terminal.supports_color()))
        .map_err(terminal_error)?;
    if let Some(notice) = runtime.startup_update_notice() {
        terminal
            .write_frame(&format!("{notice}\n\n"))
            .map_err(terminal_error)?;
    }

    let choices = model_choices(&options);
    let Some(selected_id) = terminal
        .choose("Select Model / 모델 선택", &choices)
        .map_err(terminal_error)?
    else {
        terminal
            .write_frame("초기 설정을 건너뛰었습니다. TUI에서 /model로 다시 시작할 수 있습니다.\n")
            .map_err(terminal_error)?;
        return Ok(());
    };
    if selected_id == "skip" {
        terminal
            .write_frame("초기 설정을 건너뛰었습니다. TUI에서 /model로 다시 시작할 수 있습니다.\n")
            .map_err(terminal_error)?;
        return Ok(());
    }
    let selected = options
        .iter()
        .find(|option| option.id == selected_id)
        .expect("terminal choice must originate from model options");
    if terminal
        .choose("설치 확인", &confirmation_choices(selected))
        .map_err(terminal_error)?
        .as_deref()
        != Some("install")
    {
        terminal
            .write_frame("설정을 취소했습니다. 다운로드하거나 backend를 변경하지 않았습니다.\n")
            .map_err(terminal_error)?;
        return Ok(());
    }

    write_stage(terminal, 1, "llama.cpp backend를 준비합니다")?;
    runtime.ensure_backend()?;
    write_stage(terminal, 2, "모델을 다운로드하고 SHA-256을 검증합니다")?;
    let prepared = runtime.prepare_model(&selected.id)?;
    write_stage(
        terminal,
        3,
        "모델을 기본값으로 선택하고 backend를 시작합니다",
    )?;
    runtime.start_model(&prepared)?;
    terminal
        .write_frame(&format!(
            "\n설정 완료\n- model: {}\n- context: {} tokens\n- vision: {}\n- backend: ready\n- 다음: 코딩 요청을 입력하세요.\n",
            prepared.id,
            prepared.context_tokens,
            if prepared.vision_ready {
                "ready"
            } else {
                "text-only"
            }
        ))
        .map_err(terminal_error)
}

pub(crate) fn render_setup_screen(options: &[TuiModelOption], color: bool) -> String {
    let mut output = String::new();
    output.push_str(&paint("rpotato 첫 실행 설정\n", "\u{001b}[1;36m", color));
    output.push_str("backend와 GGUF 경로는 자동으로 관리됩니다. 사용할 모델만 선택하세요.\n\n");
    for (index, option) in options.iter().enumerate() {
        let recommendation = if option.recommended { " [권장]" } else { "" };
        output.push_str(&format!(
            "{}. {}{}\n   id {} | {} | download {} | context {} | RAM {} | {}\n   {}\n",
            index + 1,
            option.display_name,
            recommendation,
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
        ));
    }
    output.push('\n');
    output
}

fn model_choices(options: &[TuiModelOption]) -> Vec<TerminalChoice> {
    let mut choices = options
        .iter()
        .map(|option| TerminalChoice {
            value: option.id.clone(),
            label: option.display_name.clone(),
            description: format!(
                "{} · download {} · context {} · RAM {} · {}",
                option.quantization,
                bytes_label(option.download_bytes),
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

fn confirmation_choices(selected: &TuiModelOption) -> [TerminalChoice; 2] {
    [
        TerminalChoice {
            value: "install".to_string(),
            label: "설치하고 시작".to_string(),
            description: format!(
                "{} · {} · {}",
                selected.display_name,
                bytes_label(selected.download_bytes),
                selected.license
            ),
            current: true,
            recommended: true,
        },
        TerminalChoice {
            value: "cancel".to_string(),
            label: "취소".to_string(),
            description: "다운로드하거나 backend를 변경하지 않습니다.".to_string(),
            current: false,
            recommended: false,
        },
    ]
}

fn write_stage(terminal: &mut impl TerminalIo, step: u8, label: &str) -> Result<(), AppError> {
    terminal
        .write_frame(&format!("\n[{step}/3] {label}...\n"))
        .map_err(terminal_error)
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

fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{value}\u{001b}[0m")
    } else {
        value.to_string()
    }
}

fn terminal_error(fault: TerminalFault) -> AppError {
    AppError::runtime(format!("초기 설정 terminal I/O 실패: {fault:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedTerminal {
        lines: std::collections::VecDeque<String>,
        frames: Vec<String>,
    }

    impl ScriptedTerminal {
        fn new(lines: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                lines: lines.into_iter().map(str::to_string).collect(),
                frames: Vec::new(),
            }
        }
    }

    impl TerminalIo for ScriptedTerminal {
        fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
            Ok((80, 24))
        }

        fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
            Ok(self.lines.pop_front())
        }

        fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
            self.read_line()
        }

        fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
            self.frames.push(frame.to_string());
            Ok(())
        }
    }

    struct SetupRuntime {
        calls: Vec<String>,
        startup_notice: Option<String>,
    }

    impl TuiSetupPort for SetupRuntime {
        fn startup_update_notice(&mut self) -> Option<String> {
            self.startup_notice.take()
        }

        fn model_options(&mut self) -> Vec<TuiModelOption> {
            sample_options()
        }

        fn ensure_backend(&mut self) -> Result<String, AppError> {
            self.calls.push("backend".to_string());
            Ok("ready".to_string())
        }

        fn prepare_model(&mut self, id: &str) -> Result<PreparedTuiModel, AppError> {
            self.calls.push(format!("model:{id}"));
            Ok(PreparedTuiModel {
                id: id.to_string(),
                artifact_path: "/tmp/model.gguf".to_string(),
                context_tokens: 131_072,
                vision_ready: true,
            })
        }

        fn start_model(&mut self, model: &PreparedTuiModel) -> Result<String, AppError> {
            self.calls.push(format!("start:{}", model.id));
            Ok("running".to_string())
        }
    }

    #[test]
    fn setup_lists_model_facts_and_runs_selected_pipeline() {
        let mut terminal = ScriptedTerminal::new(["2", "1"]);
        let mut runtime = SetupRuntime {
            calls: Vec::new(),
            startup_notice: None,
        };

        run_setup(&mut terminal, &mut runtime).unwrap();

        let output = terminal.frames.concat();
        assert!(output.contains("download 4.8 GiB"));
        assert!(output.contains("context 131k"));
        assert!(output.contains("RAM 미확정"));
        assert!(output.contains("Apache-2.0"));
        assert!(output.contains("설정 완료"));
        assert!(output.contains("context: 131072 tokens"));
        assert!(output.contains("vision: ready"));
        assert_eq!(
            runtime.calls,
            ["backend", "model:gemma-4-e4b", "start:gemma-4-e4b"]
        );
    }

    #[test]
    fn setup_skip_has_no_install_side_effects() {
        let mut terminal = ScriptedTerminal::new(["skip"]);
        let mut runtime = SetupRuntime {
            calls: Vec::new(),
            startup_notice: None,
        };

        run_setup(&mut terminal, &mut runtime).unwrap();

        assert!(terminal.frames.concat().contains("건너뛰었습니다"));
        assert!(runtime.calls.is_empty());
    }

    #[test]
    fn setup_renders_before_checking_and_shows_update_before_selection() {
        let mut terminal = ScriptedTerminal::new(["skip"]);
        let mut runtime = SetupRuntime {
            calls: Vec::new(),
            startup_notice: Some("새 rpotato 버전이 있습니다: v9.0.0".to_string()),
        };

        run_setup(&mut terminal, &mut runtime).unwrap();

        assert!(terminal.frames[0].contains("rpotato 첫 실행 설정"));
        assert!(!terminal.frames[0].contains("새 rpotato 버전"));
        assert!(terminal.frames[1].contains("새 rpotato 버전이 있습니다"));
        assert!(terminal.frames[2].contains("Select Model / 모델 선택"));
    }

    fn sample_options() -> Vec<TuiModelOption> {
        vec![
            TuiModelOption {
                id: "qwen3.5-4b".to_string(),
                display_name: "Qwen 4B".to_string(),
                quantization: "Q4_K_M".to_string(),
                download_bytes: 2_740_937_888,
                context_length: Some(262_144),
                ram: "미확정".to_string(),
                license: "Apache-2.0".to_string(),
                note: "실험적".to_string(),
                current: false,
                recommended: false,
            },
            TuiModelOption {
                id: "gemma-4-e4b".to_string(),
                display_name: "Gemma 4B".to_string(),
                quantization: "QAT q4_0".to_string(),
                download_bytes: 5_154_939_136,
                context_length: Some(131_072),
                ram: "미확정".to_string(),
                license: "Apache-2.0".to_string(),
                note: "local smoke".to_string(),
                current: false,
                recommended: true,
            },
        ]
    }
}
