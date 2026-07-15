use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
#[allow(dead_code, unused_imports)]
#[path = "support/native_terminal.rs"]
mod native_terminal_support;

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rpotato-interactive-{name}-{nonce}"));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        Self {
            root,
            project,
            data,
        }
    }

    fn command(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rpotato"))
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn bare_tui_on_pipe_stays_one_shot() {
    let fixture = Fixture::new("bare");
    let output = fixture.command(&["tui"], b"");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("rpotato TUI beta - overview"));
    assert!(!stdout.contains("rpotato>"));
}

#[test]
fn explicit_interactive_accepts_pipe_quit_and_eof() {
    for (name, input) in [("quit", b"quit\n".as_slice()), ("eof", b"".as_slice())] {
        let fixture = Fixture::new(name);
        let output = fixture.command(&["tui", "interactive"], input);

        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("rpotato interactive | overview"));
        assert!(stdout.contains("rpotato>"));
        assert!(!stdout.contains('\u{001b}'));
    }
}

#[test]
fn unknown_interactive_command_is_a_read_only_help_noop() {
    let fixture = Fixture::new("unknown");
    let output = fixture.command(&["tui", "interactive"], b"arbitrary shell command\nquit\n");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("알 수 없는 명령입니다"));
    assert!(!fixture.project.join("arbitrary").exists());
}

#[cfg(unix)]
#[test]
fn interactive_tui_recovery_outcome_matrix_exact() {
    let fixture = native_terminal_support::NativeTerminalFixture::new("outcome-matrix");
    let denied = fixture.prepare_source_approval();
    let denial = run_interactive(
        &fixture.project,
        &fixture.data,
        &format!(
            "select {}\ndeny\nyes\ndeny\nyes\nquit\n",
            denied.workflow_id
        ),
    );
    assert!(denial.status.success());
    let denial = normalized_output(&denial.stdout);
    assert_exact_dynamic_outcome(&denial, "deny.patch.accepted", &denied.workflow_id, None);
    assert_exact_dynamic_outcome(
        &denial,
        "deny.blocked.terminal-state",
        &denied.workflow_id,
        Some("cancelled"),
    );

    let resumed = fixture.prepare_source_approval();
    let lifecycle = run_interactive(
        &fixture.project,
        &fixture.data,
        &format!(
            "select {}\nresume\nyes\ncancel\nyes\ncancel\nyes\nquit\n",
            resumed.workflow_id
        ),
    );
    assert!(lifecycle.status.success());
    let lifecycle = normalized_output(&lifecycle.stdout);
    assert_exact_dynamic_outcome(&lifecycle, "resume.accepted", &resumed.workflow_id, None);
    assert_exact_dynamic_outcome(&lifecycle, "cancel.accepted", &resumed.workflow_id, None);
    assert_exact_dynamic_outcome(
        &lifecycle,
        "cancel.terminal-blocked",
        &resumed.workflow_id,
        Some("cancelled"),
    );
}

#[cfg(unix)]
fn run_interactive(project: &std::path::Path, data: &std::path::Path, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rpotato"))
        .args(["tui", "interactive"])
        .env("RPOTATO_PROJECT_ROOT", project)
        .env("RPOTATO_DATA_HOME", data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[cfg(unix)]
fn normalized_output(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|line| {
            line.strip_prefix("notice: ")
                .or_else(|| line.strip_prefix("        "))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
fn assert_exact_dynamic_outcome(output: &str, code: &str, workflow_id: &str, phase: Option<&str>) {
    let expected = match code {
        "deny.patch.accepted" => {
            let intent = dynamic_intent(output, code);
            format!(
                "패치 적용 거부 완료\n- code: deny.patch.accepted\n- intent: {intent}\n- workflow: {workflow_id}\n- 동작: 소스 변경 없이 취소 상태를 기록했습니다.\n- 다음: 거부 영수증을 확인하세요."
            )
        }
        "deny.blocked.terminal-state" => {
            let intent = dynamic_intent(output, code);
            format!(
                "종료 상태여서 거부 차단\n- code: deny.blocked.terminal-state\n- intent: {intent}\n- workflow: {workflow_id}\n- phase: {}\n- 동작: 종료 상태와 영수증을 변경하지 않았습니다.\n- 다음: 기존 종료 영수증을 확인하세요.",
                phase.unwrap()
            )
        }
        "resume.accepted" => {
            let intent = dynamic_intent(output, code);
            format!(
                "워크플로 재개 완료\n- code: resume.accepted\n- intent: {intent}\n- workflow: {workflow_id}\n- 동작: 검증된 정본 상태에서 재개했습니다.\n- 다음: 정본 상태를 새로고침하세요."
            )
        }
        "cancel.accepted" => {
            let intent = dynamic_intent(output, code);
            format!(
                "워크플로 취소 완료\n- code: cancel.accepted\n- intent: {intent}\n- workflow: {workflow_id}\n- 동작: 취소 상태를 기록했습니다.\n- 다음: 정본 상태를 새로고침하세요."
            )
        }
        "cancel.terminal-blocked" => format!(
            "종료된 워크플로는 취소할 수 없음\n- code: cancel.terminal-blocked\n- workflow: {workflow_id}\n- phase: {}\n- 동작: 종료 상태를 유지했습니다.\n- 다음: 종료 영수증을 확인하세요.",
            phase.unwrap()
        ),
        other => panic!("missing integration outcome oracle: {other}"),
    };
    assert_exact_outcome_block(output, &expected, code);
}

#[cfg(unix)]
fn assert_exact_outcome_block(output: &str, expected: &str, code: &str) {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let marker = expected_lines[1];
    let output_lines = output.lines().collect::<Vec<_>>();
    let matches = output_lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_start() == marker)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "outcome marker count for {code}: {matches:?}"
    );
    let start = matches[0].checked_sub(1).expect("outcome header");
    let end = start + expected_lines.len();
    assert!(end <= output_lines.len(), "truncated outcome for {code}");
    let mut actual_lines = output_lines[start..end].to_vec();
    if actual_lines[0] != expected_lines[0] && actual_lines[0].ends_with(expected_lines[0]) {
        actual_lines[0] = expected_lines[0];
    }
    assert_eq!(
        actual_lines.join("\n"),
        expected,
        "interactive exact outcome mismatch for {code}\noutput:\n{output}"
    );
}

#[cfg(unix)]
fn dynamic_intent(output: &str, code: &str) -> String {
    let intent = output_field_after_code(output, code, "intent");
    assert!(intent.starts_with("intent-tui-"));
    intent
}

#[cfg(unix)]
fn output_field_after_code(output: &str, code: &str, field: &str) -> String {
    let marker = format!("- code: {code}");
    let tail = output
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing outcome marker {marker:?}; output:\n{output}"))
        .1;
    let prefix = format!("- {field}: ");
    tail.lines()
        .find_map(|line| line.trim_start().strip_prefix(&prefix))
        .filter(|value| !value.is_empty())
        .unwrap()
        .to_string()
}
