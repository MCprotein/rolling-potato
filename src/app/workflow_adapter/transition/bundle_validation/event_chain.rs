use super::*;

pub(in super::super) fn validate_event_chain(
    bundle: &PreparedSourceBundle,
) -> Result<(), AppError> {
    if bundle.semantic_events.len() != bundle.event_chain_plan.len()
        || bundle.semantic_events.len() > 10
    {
        return Err(AppError::blocked(
            "prepared semantic event/chain cardinality 불일치",
        ));
    }
    let mut aggregate_event_bytes = 0_usize;
    for event in &bundle.semantic_events {
        let rendered = render_semantic_event(event);
        enforce_byte_limit(
            rendered.len(),
            MAX_PREPARED_EVENT_BYTES,
            "prepared semantic event byte limit 초과",
        )?;
        aggregate_event_bytes = checked_add_bytes(
            aggregate_event_bytes,
            rendered.len(),
            MAX_PREPARED_EVENTS_BYTES,
            "prepared semantic event byte count overflow",
            "prepared semantic events aggregate byte limit 초과",
        )?;
    }
    let mut previous = bundle.ledger_binding.event_hash.clone();
    let mut ids = std::collections::BTreeSet::new();
    for (index, (event, chain)) in bundle
        .semantic_events
        .iter()
        .zip(&bundle.event_chain_plan)
        .enumerate()
    {
        validate_ascii_id(&event.event_id, "event")?;
        if event.event_type.is_empty()
            || event.project_id != bundle.project_id
            || event.session_id != bundle.session_id
            || !ids.insert(event.event_id.as_str())
        {
            return Err(AppError::blocked(
                "prepared semantic event owner/id binding 불일치",
            ));
        }
        let expected_ordinal = bundle
            .ledger_binding
            .event_count
            .checked_add(
                u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("prepared event ordinal overflow"))?,
            )
            .ok_or_else(|| AppError::blocked("prepared event ordinal overflow"))?;
        let expected_hash =
            crate::app::workflow_adapter::ledger::planned_event_hash(event, &previous);
        if chain.event_id != event.event_id
            || chain.ordinal != expected_ordinal
            || chain.previous_event_hash != previous
            || chain.event_hash != expected_hash
            || !is_sha256(&chain.event_hash)
        {
            return Err(AppError::blocked(
                "prepared semantic event chain binding 불일치",
            ));
        }
        previous = chain.event_hash.clone();
    }
    Ok(())
}
