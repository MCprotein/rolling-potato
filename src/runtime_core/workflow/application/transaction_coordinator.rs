//! Ordered transaction progress independent of concrete persistence adapters.

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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str) -> LedgerEvent {
        LedgerEvent {
            event_id: id.to_owned(),
            ts_ms: 1,
            event_type: "workflow.test".to_owned(),
            project_id: "project".to_owned(),
            session_id: "session".to_owned(),
            summary: "summary".to_owned(),
            details: "details".to_owned(),
        }
    }

    fn planned(event: LedgerEvent, ordinal: u64) -> PlannedEvent {
        PlannedEvent {
            event,
            ordinal,
            previous_event_hash: "previous".to_owned(),
            event_hash: "current".to_owned(),
        }
    }

    #[test]
    fn accepts_only_the_next_bound_event() {
        let first = event("first");
        let second = event("second");
        let plan = [planned(first.clone(), 1), planned(second.clone(), 2)];
        let mut coordinator = TransactionCoordinator::new(&plan);

        assert!(coordinator.validate_next(1, &second).is_err());
        assert!(coordinator.validate_next(0, &second).is_err());
        assert_eq!(coordinator.validate_next(0, &first).unwrap(), &plan[0]);
        coordinator.record_appended(0).unwrap();
        assert!(coordinator.validate_next(0, &first).is_err());
        assert_eq!(coordinator.validate_next(1, &second).unwrap(), &plan[1]);
    }

    #[test]
    fn finishes_only_after_every_planned_event_is_recorded() {
        let first = event("first");
        let second = event("second");
        let plan = [planned(first.clone(), 1), planned(second.clone(), 2)];
        let mut coordinator = TransactionCoordinator::new(&plan);

        assert!(coordinator.finish().is_err());
        coordinator.validate_next(0, &first).unwrap();
        coordinator.record_appended(0).unwrap();
        assert!(coordinator.finish().is_err());
        coordinator.validate_next(1, &second).unwrap();
        coordinator.record_appended(1).unwrap();
        coordinator.finish().unwrap();
    }
}
