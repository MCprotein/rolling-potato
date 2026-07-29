use super::super::*;

pub(crate) fn planned_events(
    bundle: &PreparedSourceBundle,
) -> Result<Vec<crate::app::workflow_adapter::ledger::PlannedEvent>, AppError> {
    validate_prepared_source_bundle(bundle)?;
    Ok(bundle
        .semantic_events
        .iter()
        .cloned()
        .zip(bundle.event_chain_plan.iter())
        .map(
            |(event, chain)| crate::app::workflow_adapter::ledger::PlannedEvent {
                event,
                ordinal: chain.ordinal,
                previous_event_hash: chain.previous_event_hash.clone(),
                event_hash: chain.event_hash.clone(),
            },
        )
        .collect())
}

pub(crate) fn bind_planned_events(
    bundle: &mut PreparedSourceBundle,
    planned: &[crate::app::workflow_adapter::ledger::PlannedEvent],
) -> Result<(), AppError> {
    bundle.semantic_events = planned.iter().map(|entry| entry.event.clone()).collect();
    bundle.event_chain_plan = planned
        .iter()
        .map(|entry| PreparedEventChain {
            event_id: entry.event.event_id.clone(),
            ordinal: entry.ordinal,
            previous_event_hash: entry.previous_event_hash.clone(),
            event_hash: entry.event_hash.clone(),
        })
        .collect();
    validate_event_chain(bundle)
}
