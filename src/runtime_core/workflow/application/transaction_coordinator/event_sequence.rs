//! Exact semantic-event sequence validation for persistence adapters.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::LedgerEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedEvent {
    pub event: LedgerEvent,
    pub ordinal: u64,
    pub previous_event_hash: String,
    pub event_hash: String,
}

pub(crate) struct TransactionCoordinator<'plan> {
    planned: &'plan [PlannedEvent],
    next_index: usize,
}

impl<'plan> TransactionCoordinator<'plan> {
    pub(crate) fn new(planned: &'plan [PlannedEvent]) -> Self {
        Self {
            planned,
            next_index: 0,
        }
    }

    pub(crate) fn validate_next(
        &self,
        index: usize,
        event: &LedgerEvent,
    ) -> Result<&'plan PlannedEvent, AppError> {
        if index != self.next_index {
            return Err(AppError::blocked(format!(
                "transaction event sink 순서 불일치\n- expected index: {}\n- requested index: {index}",
                self.next_index
            )));
        }
        let planned = self
            .planned
            .get(index)
            .ok_or_else(|| AppError::blocked("transaction event sink index 범위 초과"))?;
        if &planned.event != event {
            return Err(AppError::blocked(
                "transaction event sink semantic event binding 불일치",
            ));
        }
        Ok(planned)
    }

    pub(crate) fn record_appended(&mut self, index: usize) -> Result<(), AppError> {
        if index != self.next_index {
            return Err(AppError::blocked(format!(
                "transaction event sink 순서 불일치\n- expected index: {}\n- requested index: {index}",
                self.next_index
            )));
        }
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("transaction event sink index overflow"))?;
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<(), AppError> {
        if self.next_index != self.planned.len() {
            return Err(AppError::blocked(format!(
                "transaction event sink 미완료\n- appended: {}\n- planned: {}",
                self.next_index,
                self.planned.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn planned(&self) -> &'plan [PlannedEvent] {
        self.planned
    }
}
