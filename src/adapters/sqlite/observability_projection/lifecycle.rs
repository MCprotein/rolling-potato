use super::*;

pub fn initialize(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    status_from_connection(&connection, recovered_from)
}

pub fn status(ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    status_from_connection(&connection, recovered_from)
}

pub(crate) fn project_event_with_ordinal(
    event: &LedgerEvent,
    ordinal: u64,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let ordinal = i64::try_from(ordinal)
        .map_err(|_| AppError::blocked("observability event ordinal 범위 초과"))?;
    let (connection, _) = open_or_recover()?;
    insert_ledger_event(&connection, event, ordinal, ledger)
}

pub(crate) fn converge_from_events(
    events: &[ParsedLedgerEvent],
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, events, ledger)
}
