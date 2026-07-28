use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::TuiEffect;
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, TuiIntent, TuiSessionOption, TuiSessionTransition,
};

use super::super::{canonical_dispatch_intent, canonical_selection_lease};

pub(in crate::app::tui_adapter) struct TuiRuntimeAdapter {
    pub(super) web_pages: crate::app::web_search_adapter::WebPageSession,
    pub(super) conversation_memory: Option<super::super::session_memory::ConversationMemory>,
    pub(super) fresh_session_pending: bool,
}

impl Default for TuiRuntimeAdapter {
    fn default() -> Self {
        Self {
            web_pages: crate::app::web_search_adapter::WebPageSession::default(),
            conversation_memory: None,
            fresh_session_pending: true,
        }
    }
}

impl TuiRuntimeAdapter {
    #[cfg(test)]
    pub(in crate::app::tui_adapter) fn mark_session_active_for_test(&mut self) {
        self.fresh_session_pending = false;
    }

    pub(super) fn ensure_fresh_session(&mut self) -> Result<(), AppError> {
        if !self.fresh_session_pending {
            return Ok(());
        }
        crate::app::workflow_adapter::state::session_new_report()?;
        self.conversation_memory = None;
        self.web_pages.clear();
        self.fresh_session_pending = false;
        Ok(())
    }

    pub(super) fn conversation_memory(
        &mut self,
    ) -> Result<&mut super::super::session_memory::ConversationMemory, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        if !self
            .conversation_memory
            .as_ref()
            .is_some_and(|memory| memory.belongs_to(&identity.session_id))
        {
            self.conversation_memory = Some(super::super::session_memory::load()?);
        }
        self.conversation_memory
            .as_mut()
            .ok_or_else(|| AppError::blocked("conversation memory 초기화 실패"))
    }

    pub(super) fn available_sessions(&mut self) -> Result<Vec<TuiSessionOption>, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        Ok(crate::app::observability_adapter::session_history(20)?
            .into_iter()
            .map(|session| TuiSessionOption {
                current: !self.fresh_session_pending && session.session_id == identity.session_id,
                preview: session
                    .last_summary
                    .unwrap_or_else(|| "저장된 대화".to_string()),
                session_id: session.session_id,
            })
            .collect())
    }

    pub(super) fn start_session(&mut self) -> Result<TuiSessionTransition, AppError> {
        crate::app::workflow_adapter::state::session_new_report()?;
        self.conversation_memory = None;
        self.web_pages.clear();
        self.fresh_session_pending = false;
        self.session_transition("새 세션을 시작했습니다.")
    }

    pub(super) fn resume_selected_session(
        &mut self,
        session_id: &str,
    ) -> Result<TuiSessionTransition, AppError> {
        let outcome = canonical_dispatch_intent(TuiIntent::ResumeSession {
            intent_id: new_tui_intent_id(),
            session_id: session_id.to_string(),
            lease: canonical_selection_lease(session_id)?,
        })?;
        if outcome.effect != TuiEffect::Committed {
            return Err(AppError::blocked(outcome.safe_message));
        }
        self.conversation_memory = None;
        self.web_pages.clear();
        self.fresh_session_pending = false;
        self.session_transition("선택한 세션을 재개했습니다.")
    }

    fn session_transition(&mut self, notice: &str) -> Result<TuiSessionTransition, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        Ok(TuiSessionTransition {
            session_id: identity.session_id,
            notice: notice.to_string(),
            turns: self.conversation_memory()?.turns.clone(),
        })
    }
}
