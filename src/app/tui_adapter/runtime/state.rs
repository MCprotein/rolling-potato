use crate::foundation::error::AppError;

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
}
