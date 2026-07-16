//! Workflow runtime orchestration over explicit application ports.

use crate::foundation::error::AppError;
use crate::runtime_core::reporting::runtime_report::{
    self, SessionResumeReport, WorkflowResumeReport,
};

pub(crate) trait RuntimeApplicationPort {
    fn run_agent(&mut self, request: &str) -> Result<String, AppError>;
    fn current_session_id(&mut self) -> Result<String, AppError>;
    fn rebuild_resume_context(&mut self, session_id: &str) -> Result<String, AppError>;
    fn resume_report(&mut self) -> Result<String, AppError>;
    fn session_resume_preflight(&mut self, session_id: &str) -> Result<Option<String>, AppError>;
    fn preflight_workflow(&mut self, workflow_id: &str) -> Result<(), AppError>;
    fn session_resume_report(&mut self, session_id: &str) -> Result<String, AppError>;
    fn approve_patch(
        &mut self,
        proposal_id: &str,
        token: &str,
        dry_run: bool,
        verify_command: Option<&str>,
    ) -> Result<(), AppError>;
    fn verify_patch(&mut self, proposal_id: &str, token: &str) -> Result<String, AppError>;
}

pub(crate) fn agent_run_report(
    port: &mut impl RuntimeApplicationPort,
    request: &str,
) -> Result<String, AppError> {
    port.run_agent(request)
}

pub(crate) fn workflow_resume_report(
    port: &mut impl RuntimeApplicationPort,
) -> Result<String, AppError> {
    let session_id = port.current_session_id()?;
    let reconstructed_context = port.rebuild_resume_context(&session_id)?;
    let continuation = port.resume_report()?;
    Ok(runtime_report::render_workflow_resume(
        WorkflowResumeReport {
            continuation,
            reconstructed_context,
        },
    ))
}

pub(crate) fn session_resume_report(
    port: &mut impl RuntimeApplicationPort,
    session_id: &str,
) -> Result<String, AppError> {
    let reconstructed_context = port.rebuild_resume_context(session_id)?;
    if let Some(workflow_id) = port.session_resume_preflight(session_id)? {
        port.preflight_workflow(&workflow_id)?;
    }
    let selection = port.session_resume_report(session_id)?;
    let continuation = port.resume_report()?;
    Ok(runtime_report::render_session_resume(SessionResumeReport {
        selection,
        reconstructed_context,
        continuation,
    }))
}

pub(crate) fn patch_approve_to_stdout(
    port: &mut impl RuntimeApplicationPort,
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<(), AppError> {
    port.approve_patch(proposal_id, token, dry_run, verify_command)
}

pub(crate) fn patch_verify_report(
    port: &mut impl RuntimeApplicationPort,
    proposal_id: &str,
    token: &str,
) -> Result<String, AppError> {
    Ok(runtime_report::guard_patch_terminal(
        port.verify_patch(proposal_id, token)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakePort {
        calls: Vec<String>,
        resume_workflow: Option<String>,
        approval: Option<(String, String, bool, Option<String>)>,
    }

    impl RuntimeApplicationPort for FakePort {
        fn run_agent(&mut self, request: &str) -> Result<String, AppError> {
            self.calls.push(format!("run-agent:{request}"));
            Ok(format!("agent:{request}"))
        }

        fn current_session_id(&mut self) -> Result<String, AppError> {
            self.calls.push("current-session".into());
            Ok("session-current".into())
        }

        fn rebuild_resume_context(&mut self, session_id: &str) -> Result<String, AppError> {
            self.calls.push(format!("rebuild-context:{session_id}"));
            Ok(format!("context:{session_id}"))
        }

        fn resume_report(&mut self) -> Result<String, AppError> {
            self.calls.push("resume-report".into());
            Ok("patch approve\n- status: pending".into())
        }

        fn session_resume_preflight(
            &mut self,
            session_id: &str,
        ) -> Result<Option<String>, AppError> {
            self.calls.push(format!("session-preflight:{session_id}"));
            Ok(self.resume_workflow.clone())
        }

        fn preflight_workflow(&mut self, workflow_id: &str) -> Result<(), AppError> {
            self.calls.push(format!("workflow-preflight:{workflow_id}"));
            Ok(())
        }

        fn session_resume_report(&mut self, session_id: &str) -> Result<String, AppError> {
            self.calls.push(format!("session-report:{session_id}"));
            Ok(format!("session resume\n- session id: {session_id}"))
        }

        fn approve_patch(
            &mut self,
            proposal_id: &str,
            token: &str,
            dry_run: bool,
            verify_command: Option<&str>,
        ) -> Result<(), AppError> {
            self.calls.push("approve-patch".into());
            self.approval = Some((
                proposal_id.to_string(),
                token.to_string(),
                dry_run,
                verify_command.map(str::to_string),
            ));
            Ok(())
        }

        fn verify_patch(&mut self, proposal_id: &str, token: &str) -> Result<String, AppError> {
            self.calls
                .push(format!("verify-patch:{proposal_id}:{token}"));
            Ok("패치 작업 완료\nSummary\n- 결과: 성공".into())
        }
    }

    #[test]
    fn resume_orchestration_order_and_reports_are_stable() {
        let mut workflow = FakePort::default();
        assert_eq!(
            workflow_resume_report(&mut workflow).unwrap(),
            "patch approve\n- status: pending\n- reconstructed context: context:session-current"
        );
        assert_eq!(
            workflow.calls,
            [
                "current-session",
                "rebuild-context:session-current",
                "resume-report"
            ]
        );

        let mut session = FakePort {
            resume_workflow: Some("workflow-1".into()),
            ..FakePort::default()
        };
        assert_eq!(
            session_resume_report(&mut session, "session-1").unwrap(),
            "session resume\n- session id: session-1\n- reconstructed context: context:session-1\n- continuation:\npatch approve\n- status: pending"
        );
        assert_eq!(
            session.calls,
            [
                "rebuild-context:session-1",
                "session-preflight:session-1",
                "workflow-preflight:workflow-1",
                "session-report:session-1",
                "resume-report"
            ]
        );
    }

    #[test]
    fn patch_and_agent_arguments_are_preserved() {
        let mut port = FakePort::default();
        assert_eq!(agent_run_report(&mut port, "요청").unwrap(), "agent:요청");
        patch_approve_to_stdout(&mut port, "proposal-1", "token-1", true, Some("cargo test"))
            .unwrap();
        assert_eq!(
            port.approval,
            Some((
                "proposal-1".into(),
                "token-1".into(),
                true,
                Some("cargo test".into())
            ))
        );
        assert_eq!(
            patch_verify_report(&mut port, "proposal-1", "token-1").unwrap(),
            "패치 작업 완료\n- 결과: 성공"
        );
    }
}
