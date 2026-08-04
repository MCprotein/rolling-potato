use super::super::*;
use crate::surfaces::tui::runtime_bridge::TuiVisionStatus;

#[test]
fn local_access_status_uses_verified_runtime_capability_without_stealing_tasks() {
    for request in [
        "너 로컬 파일시스템 접근 가능해?",
        "현재 프로젝트 파일을 직접 읽을 수 있어?",
    ] {
        let reply = local_reply(request, Some("gemma-test"), TuiVisionStatus::OnDemand)
            .expect("local access capability is a runtime fact");
        assert!(reply.starts_with("가능합니다."), "{request}");
        assert!(reply.contains("현재 프로젝트 범위"), "{request}");
        assert!(reply.contains("읽기 전용"), "{request}");
    }

    assert_eq!(
        local_reply(
            "src/main.rs 파일을 읽어서 구조를 분석해줘",
            Some("gemma-test"),
            TuiVisionStatus::OnDemand,
        ),
        None,
    );
}
