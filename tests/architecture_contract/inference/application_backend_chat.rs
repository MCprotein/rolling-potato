use super::*;

pub(super) fn assert_backend_chat_owners(backend_adapter: &str) {
    let chat_path = "src/app/inference_adapter/backend/chat.rs";
    let execution_path = "src/app/inference_adapter/backend/chat/execution.rs";
    let interruption_path = "src/app/inference_adapter/backend/chat/interruption.rs";
    let readiness_path = "src/app/inference_adapter/backend/chat/readiness.rs";
    let report_path = "src/app/inference_adapter/backend/chat/report.rs";
    for path in [
        chat_path,
        execution_path,
        interruption_path,
        readiness_path,
        report_path,
    ] {
        assert!(Path::new(path).is_file(), "missing chat owner: {path}");
    }

    let chat = fs::read_to_string(chat_path).unwrap();
    let execution = fs::read_to_string(execution_path).unwrap();
    let interruption = fs::read_to_string(interruption_path).unwrap();
    let readiness = fs::read_to_string(readiness_path).unwrap();
    let report = fs::read_to_string(report_path).unwrap();

    assert!(
        backend_adapter.lines().any(|line| line == "mod chat;"),
        "inference backend adapter does not register its chat owner"
    );
    for owner in ["execution", "interruption", "readiness", "report"] {
        assert!(
            chat.lines().any(|line| line == format!("mod {owner};")),
            "backend chat facade does not register {owner}"
        );
    }
    assert_moved_responsibilities(
        &report,
        &chat,
        &[
            "pub fn chat_report(",
            "pub fn chat_stream_report(",
            "fn format_chat_run(",
        ],
        "report",
    );
    assert!(report.contains("fn chat_report_format_preserves_diagnostics_and_response_boundary("));
    for responsibility in [
        "pub fn chat_once(",
        "pub fn chat_once_bounded(",
        "pub fn chat_once_bounded_with_cancel(",
        "pub fn preflight_chat_ready(",
        "fn chat_once_with_options(",
    ] {
        assert!(chat.contains(responsibility));
        assert!(!backend_adapter.contains(responsibility));
    }
    assert_moved_responsibilities(
        &readiness,
        &chat,
        &["pub(super) fn ready_sidecar_record("],
        "readiness",
    );
    assert_moved_responsibilities(
        &execution,
        &chat,
        &["pub(super) fn chat_input_with_options("],
        "execution",
    );
    assert_moved_responsibilities(
        &interruption,
        &chat,
        &[
            "pub fn cancel_generation_report(",
            "pub(super) fn finish_interrupted_generation(",
        ],
        "interruption",
    );

    assert!(chat.lines().count() < 125);
    assert!(execution.lines().count() < 400);
    assert!(interruption.lines().count() < 225);
    assert!(readiness.lines().count() < 75);
    assert!(report.lines().count() < 200);
}

fn assert_moved_responsibilities(owner: &str, facade: &str, items: &[&str], label: &str) {
    for responsibility in items {
        assert!(
            owner.contains(responsibility),
            "backend chat {label} owner is missing: {responsibility}"
        );
        assert!(
            !facade.contains(responsibility),
            "backend chat facade still owns {label}: {responsibility}"
        );
    }
}
