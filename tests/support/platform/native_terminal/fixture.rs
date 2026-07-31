use super::*;

pub struct NativeTerminalFixture {
    _lock: std::sync::MutexGuard<'static, ()>,
    pub root: PathBuf,
    pub project: PathBuf,
    pub data: PathBuf,
}

pub struct PendingSourceApproval {
    pub workflow_id: String,
    pub proposal_id: String,
    pub approval_token: String,
    pub source: PathBuf,
}

pub struct PreparedConversationBackend {
    project: PathBuf,
    data: PathBuf,
    request_bodies: PathBuf,
    stopped: bool,
}

impl PreparedConversationBackend {
    pub fn request_bodies(&self) -> Vec<String> {
        std::fs::read_to_string(&self.request_bodies)
            .unwrap_or_default()
            .split("\n---RPOTATO-REQUEST---\n")
            .map(str::trim)
            .filter(|request| !request.is_empty())
            .map(str::to_string)
            .collect()
    }

    pub fn clear_request_bodies(&self) {
        std::fs::write(&self.request_bodies, b"").unwrap();
    }

    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        let _ = run_bounded_command(
            Command::new(env!("CARGO_BIN_EXE_rpotato"))
                .args(["backend", "stop"])
                .env("RPOTATO_PROJECT_ROOT", &self.project)
                .env("RPOTATO_DATA_HOME", &self.data),
            "backend stop",
            &self.data,
        );
        self.stopped = true;
    }
}

impl Drop for PreparedConversationBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

impl NativeTerminalFixture {
    pub fn new(case_name: &str) -> Self {
        let lock = NATIVE_TERMINAL_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rpotato-native-terminal-{case_name}-{}-{nonce}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        std::fs::create_dir_all(&project).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rpotato"))
            .arg("init")
            .env("RPOTATO_PROJECT_ROOT", &project)
            .env("RPOTATO_DATA_HOME", &data)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native terminal fixture init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        std::env::set_var("RPOTATO_TEST_SKIP_SETUP", "1");
        std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
        Self {
            _lock: lock,
            root,
            project,
            data,
        }
    }

    pub fn prepare_source_approval(&self) -> PendingSourceApproval {
        let source_dir = self.project.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_name = format!(
            "native_source_{}.rs",
            SOURCE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let relative_source = format!("src/{source_name}");
        let source = source_dir.join(source_name);
        std::fs::write(&source, "pub const VALUE: i32 = 1;\n").unwrap();
        let response = self.root.join("response.txt");
        std::fs::write(
            &response,
            format!(
                "수정 후보를 준비했습니다.\nMODEL ACTION: kind=patch-proposal; source_pointers={relative_source}:1; path={relative_source}; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none"
            ),
        )
        .unwrap();
        let calls = self.root.join("calls.txt");
        let backend = self.root.join(if cfg!(windows) {
            "fake-sidecar.exe"
        } else {
            "fake-sidecar"
        });
        let fake_sidecar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/platform/fake_sidecar.rs");
        let compile = Command::new("rustc")
            .arg("--edition=2021")
            .arg(fake_sidecar)
            .arg("-o")
            .arg(&backend)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "native fixture fake sidecar compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let model = self.root.join("model.gguf");
        std::fs::write(&model, b"fake model").unwrap();
        let port = native_port();
        let command = |args: &[&str]| {
            let label = args.join(" ");
            trace_stage(&format!("run {label}"));
            let output = run_bounded_command(
                Command::new(env!("CARGO_BIN_EXE_rpotato"))
                    .args(args)
                    .env("RPOTATO_PROJECT_ROOT", &self.project)
                    .env("RPOTATO_DATA_HOME", &self.data)
                    .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &backend)
                    .env("RPOTATO_BACKEND_PORT", port.to_string())
                    .env("RPOTATO_FAKE_REQUEST_MARKER", &calls)
                    .env("RPOTATO_FAKE_RESPONSE_FILE", &response)
                    .env(
                        "RPOTATO_TEST_BACKEND_START_TRACE",
                        self.data.join("logs/backend-start-trace.log"),
                    ),
                &label,
                &self.data,
            );
            trace_stage(&format!("finished {label}"));
            output
        };
        let start = command(&[
            "backend",
            "start",
            "--model",
            model.to_str().unwrap(),
            "--ctx-size",
            "1024",
        ]);
        assert!(
            start.status.success(),
            "native source fixture backend start failed\nstdout={}\nstderr={}\n{}",
            String::from_utf8_lossy(&start.stdout),
            String::from_utf8_lossy(&start.stderr),
            backend_failure_diagnostics(&self.data),
        );
        let run = command(&[
            "skill",
            "run",
            "small-patch",
            "src/lib.rs의 값을 2로 고쳐줘",
        ]);
        let _ = command(&["backend", "stop"]);
        let ledger = std::fs::read_to_string(self.data.join("state/runtime-ledger.jsonl"))
            .unwrap_or_default();
        let ledger_tail = ledger
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            run.status.success(),
            "native source fixture skill run failed\nstdout={}\nstderr={}\nledger tail={ledger_tail}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
            backend_failure_diagnostics(&self.data),
        );
        let report = String::from_utf8(run.stdout).unwrap();
        let field = |key: &str| {
            report
                .lines()
                .find_map(|line| line.strip_prefix(&format!("- {key}: ")))
                .unwrap_or_else(|| panic!("missing {key} in native fixture report"))
                .to_string()
        };
        let approval_token = report
            .lines()
            .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
            .and_then(|line| line.split(" --token ").nth(1))
            .expect("native fixture approval token")
            .to_string();
        PendingSourceApproval {
            workflow_id: field("workflow id"),
            proposal_id: field("proposal id"),
            approval_token,
            source,
        }
    }

    pub fn start_conversation_backend_with_responses(
        &self,
        structured_response_body: &str,
        text_response_body: &str,
    ) -> PreparedConversationBackend {
        let backend = self.root.join(if cfg!(windows) {
            "fake-conversation-sidecar.exe"
        } else {
            "fake-conversation-sidecar"
        });
        let fake_sidecar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/platform/fake_sidecar.rs");
        let compile = Command::new("rustc")
            .arg("--edition=2021")
            .arg(fake_sidecar)
            .arg("-o")
            .arg(&backend)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "native conversation fixture fake sidecar compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        const MODEL_ID: &str = "gemma-4-e4b";
        const MODEL_FILE: &str = "gemma-4-E4B_q4_0-it.gguf";
        const MODEL_SHA256: &str =
            "e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d";
        const MODEL_CONTEXT: &str = "131072";
        let model = self.data.join("models").join(MODEL_FILE);
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, b"fake conversation model").unwrap();
        let registry = self.data.join("models/registry");
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::write(
            registry.join(format!("{MODEL_ID}.json")),
            format!(
                "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{MODEL_ID}\",\n  \"displayName\": \"Gemma conversation fixture\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"source-backed-manifest\",\n  \"promotionEvidencePath\": \"\",\n  \"backendVersion\": \"\",\n  \"benchmarkRunId\": \"\",\n  \"upstreamModel\": \"google/gemma-4-E4B-it-qat-q4_0-unquantized\",\n  \"upstreamUrl\": \"https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-unquantized\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{MODEL_SHA256}\",\n  \"licenseSource\": \"https://ai.google.dev/gemma/apache_2\",\n  \"licenseCheckedAt\": \"2026-07-11\"\n}}\n",
                model.display()
            ),
        )
        .unwrap();
        std::fs::write(
            self.data.join("models/default.json"),
            format!(
                "{{\n  \"schemaVersion\": 1,\n  \"modelId\": \"{MODEL_ID}\",\n  \"artifactSha256\": \"{MODEL_SHA256}\",\n  \"selectedAtMs\": 1\n}}\n"
            ),
        )
        .unwrap();
        let structured_response = self.root.join("structured-response.txt");
        std::fs::write(&structured_response, structured_response_body).unwrap();
        let text_response = self.root.join("text-response.txt");
        std::fs::write(&text_response, text_response_body).unwrap();
        let request_bodies = self.root.join("conversation-request-bodies.txt");
        let port = native_port();
        let start = run_bounded_command(
            Command::new(env!("CARGO_BIN_EXE_rpotato"))
                .args([
                    "backend",
                    "start",
                    "--model",
                    model.to_str().unwrap(),
                    "--ctx-size",
                    MODEL_CONTEXT,
                ])
                .env("RPOTATO_PROJECT_ROOT", &self.project)
                .env("RPOTATO_DATA_HOME", &self.data)
                .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &backend)
                .env("RPOTATO_BACKEND_PORT", port.to_string())
                .env(
                    "RPOTATO_FAKE_STRUCTURED_RESPONSE_FILE",
                    &structured_response,
                )
                .env("RPOTATO_FAKE_TEXT_RESPONSE_FILE", &text_response)
                .env("RPOTATO_FAKE_REQUEST_BODY_MARKER", &request_bodies)
                .env(
                    "RPOTATO_TEST_BACKEND_START_TRACE",
                    self.data.join("logs/conversation-backend-start-trace.log"),
                ),
            "backend start conversation fixture",
            &self.data,
        );
        assert!(
            start.status.success(),
            "native conversation fixture backend start failed\nstdout={}\nstderr={}\n{}",
            String::from_utf8_lossy(&start.stdout),
            String::from_utf8_lossy(&start.stderr),
            backend_failure_diagnostics(&self.data),
        );

        std::env::set_var("RPOTATO_BACKEND_LLAMA_CPP_PATH", &backend);
        std::env::set_var("RPOTATO_BACKEND_PORT", port.to_string());
        std::env::set_var(
            "RPOTATO_TEST_WEB_SEARCH_HTML",
            include_str!("../../../fixtures/web_search/ddg-html.html"),
        );
        std::env::set_var(
            "RPOTATO_TEST_WEB_OPEN_HTML",
            include_str!("../../../fixtures/web_search/page-hostile.html"),
        );

        PreparedConversationBackend {
            project: self.project.clone(),
            data: self.data.clone(),
            request_bodies,
            stopped: false,
        }
    }

    #[cfg(windows)]
    pub fn current_session_id(&self) -> String {
        let body = std::fs::read_to_string(self.project.join(".rpotato/state/current-state.json"))
            .unwrap();
        body.split("\"session_id\"")
            .nth(1)
            .and_then(|tail| tail.split_once(':').map(|(_, value)| value))
            .map(str::trim_start)
            .and_then(|tail| tail.strip_prefix('"'))
            .and_then(|tail| tail.split('"').next())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("current-state session_id missing: {body}"))
            .to_string()
    }
}

impl Drop for NativeTerminalFixture {
    fn drop(&mut self) {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_TEST_TERMINAL_FAULT");
        std::env::remove_var("RPOTATO_TEST_TUI_SECRET_PROBE");
        std::env::remove_var("RPOTATO_TEST_SKIP_SETUP");
        std::env::remove_var("RPOTATO_TEST_SKIP_UPDATE_CHECK");
        std::env::remove_var("RPOTATO_TEST_WEB_SEARCH_HTML");
        std::env::remove_var("RPOTATO_TEST_WEB_OPEN_HTML");
        std::env::remove_var("RPOTATO_BACKEND_LLAMA_CPP_PATH");
        std::env::remove_var("RPOTATO_BACKEND_PORT");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
