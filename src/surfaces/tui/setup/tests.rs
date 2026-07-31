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
            vision: TuiVisionStatus::OnDemand,
        })
    }

    fn start_model(&mut self, model: &PreparedTuiModel) -> Result<String, AppError> {
        self.calls.push(format!("start:{}", model.id));
        Ok("running".to_string())
    }
}

#[test]
fn setup_lists_model_facts_and_runs_selected_pipeline() {
    let mut terminal = ScriptedTerminal::new(["2", "2"]);
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
    assert!(output.contains("vision: on-demand"));
    assert_eq!(
        runtime.calls,
        ["backend", "model:gemma-4-e4b", "start:gemma-4-e4b"]
    );
}

#[test]
fn setup_reuses_a_cached_model_without_claiming_a_new_download() {
    struct CachedSetupRuntime {
        calls: Vec<String>,
        options: Vec<TuiModelOption>,
    }

    impl TuiSetupPort for CachedSetupRuntime {
        fn startup_update_notice(&mut self) -> Option<String> {
            None
        }

        fn model_options(&mut self) -> Vec<TuiModelOption> {
            self.options.clone()
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
                vision: TuiVisionStatus::OnDemand,
            })
        }

        fn start_model(&mut self, model: &PreparedTuiModel) -> Result<String, AppError> {
            self.calls.push(format!("start:{}", model.id));
            Ok("running".to_string())
        }
    }

    let mut options = sample_options();
    options[1].model_cached = true;
    let mut terminal = ScriptedTerminal::new(["2", "2"]);
    let mut runtime = CachedSetupRuntime {
        calls: Vec::new(),
        options,
    };

    run_setup(&mut terminal, &mut runtime).unwrap();

    let output = terminal.frames.concat();
    assert!(output.contains("기존 모델로 시작"));
    assert!(output.contains("기존 모델 cache를 SHA-256 검증"));
    assert!(!output.contains("[2/3] 모델을 다운로드"));
}

#[test]
fn setup_confirmation_defaults_to_cancel_without_install_side_effects() {
    let mut terminal = ScriptedTerminal::new(["2", "1"]);
    let mut runtime = SetupRuntime {
        calls: Vec::new(),
        startup_notice: None,
    };

    run_setup(&mut terminal, &mut runtime).unwrap();

    let output = terminal.frames.concat();
    assert!(output.contains("설치 확인"));
    assert!(output.contains("1. 취소"));
    assert!(output.contains("2. 설치하고 시작"));
    assert!(output.contains("설정을 취소했습니다"));
    assert!(runtime.calls.is_empty());
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
            model_cached: false,
            vision_projector_bytes: Some(672_423_616),
            vision_projector_cached: false,
            context_length: Some(262_144),
            ram: "미확정".to_string(),
            license: "Apache-2.0".to_string(),
            note: "실험적".to_string(),
            current: false,
            evaluation_recommended: false,
            readiness: crate::surfaces::tui::runtime_bridge::TuiModelReadiness::EvaluationOnly,
        },
        TuiModelOption {
            id: "gemma-4-e4b".to_string(),
            display_name: "Gemma 4B".to_string(),
            quantization: "QAT q4_0".to_string(),
            download_bytes: 5_154_939_136,
            model_cached: false,
            vision_projector_bytes: Some(991_551_904),
            vision_projector_cached: false,
            context_length: Some(131_072),
            ram: "미확정".to_string(),
            license: "Apache-2.0".to_string(),
            note: "local smoke".to_string(),
            current: false,
            evaluation_recommended: true,
            readiness: crate::surfaces::tui::runtime_bridge::TuiModelReadiness::EvaluationOnly,
        },
    ]
}

#[test]
fn setup_labels_evaluation_recommendation_separately_from_runtime_readiness() {
    let options = sample_options();
    let screen = render_setup_screen(&options, false);
    let choices = model_choices(&options);
    let confirmation = confirmation_choices(&options[1]);

    assert!(screen.contains("Gemma 4B [평가 권장]"));
    assert!(screen.contains("상태: 평가 전용 · 실사용 검증 미완료"));
    assert!(choices[1].label.contains("평가 권장"));
    assert!(!choices[1].recommended);
    assert_eq!(confirmation[1].label, "평가용으로 설치하고 시작");
    assert!(confirmation[1].description.contains("실사용 검증 미완료"));
}
