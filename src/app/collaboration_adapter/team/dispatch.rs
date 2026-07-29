use super::admission::normalize_ownership_claims;
use super::report_format::*;
use super::*;

pub fn dispatch_report(
    requested_lanes: u32,
    owned_write_paths: &[(u32, String)],
    failed_lane: Option<u32>,
    failure_reason: Option<&str>,
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let lane_decision = resource::team_lane_decision(pressure, requested_lanes);
    let ownership_gate = evaluate_ownership_gate(
        lane_decision.admitted_lanes,
        normalize_ownership_claims(owned_write_paths)?,
    );
    let continuation = continuation_decision(
        lane_decision.admitted_lanes,
        failed_lane,
        &ledger::redact_text(failure_reason.unwrap_or("not-provided")),
    );
    let blocked_by_resource = lane_decision.is_blocked();
    let blocked_by_ownership = ownership_gate.is_blocked();
    let blocked_by_continuation = continuation.is_blocked();
    let dispatch_blocked = if blocked_by_resource || blocked_by_ownership || blocked_by_continuation
    {
        "yes"
    } else {
        "no"
    };
    let status = dispatch_status(lane_decision.admission, blocked_by_ownership, &continuation);
    let event = ledger::new_event_for(
        &identity,
        dispatch_event_type(
            lane_decision.admission,
            blocked_by_ownership,
            &continuation,
        ),
        dispatch_summary(
            lane_decision.admission,
            blocked_by_ownership,
            &continuation,
        ),
        &format!(
            "requested_lanes={} admitted_lanes={} admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} ownership_status={} ownership_blocked={} owned_write_paths={} failed_lane={} failure_reason={} continuation_status={} continuation_action={} continuation_remaining_lanes={} reason={}",
            lane_decision.requested_lanes,
            lane_decision.admitted_lanes,
            lane_decision.admission.as_str(),
            dispatch_blocked,
            lane_decision.fallback,
            lane_decision.pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            ownership_gate.status,
            ownership_gate.blocked_label(),
            display_owned_write_paths(owned_write_paths),
            display_optional_lane(failed_lane),
            ledger::redact_text(failure_reason.unwrap_or("not-provided")),
            continuation.status,
            continuation.action,
            continuation.remaining_lanes,
            continuation.reason
        ),
    );
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)?;

    let report = format!(
        "team dispatch\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- lane admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- ownership claims: {}\n- ownership status: {}\n- ownership blocked: {}\n- owned write paths: {}\n- ownership decisions:\n{}\n- failed lane: {}\n- failure reason: {}\n- continuation status: {}\n- continuation action: {}\n- continuation remaining lanes: {}\n- continuation reason: {}\n- continuation hint: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: dispatch preflight only; records ownership and failed-worker continuation state, but does not start subagents, execute tools, merge files, or advance team stages.",
        status,
        store.path.display(),
        identity.session_id,
        lane_decision.requested_lanes,
        lane_decision.admitted_lanes,
        lane_decision.admission.as_str(),
        dispatch_blocked,
        lane_decision.fallback,
        ownership_gate.checks.len(),
        ownership_gate.status,
        ownership_gate.blocked_label(),
        display_owned_write_paths(owned_write_paths),
        format_ownership_checks(&ownership_gate.checks),
        display_optional_lane(failed_lane),
        ledger::redact_text(failure_reason.unwrap_or("not-provided")),
        continuation.status,
        continuation.action,
        continuation.remaining_lanes,
        continuation.reason,
        continuation.hint,
        if sample.is_some() {
            "latest-resource-sample"
        } else {
            "no-sample"
        },
        sample
            .as_ref()
            .map(|sample| sample.resource_sample_id.as_str())
            .unwrap_or("없음"),
        sample
            .as_ref()
            .map(|sample| sample.recorded_at_ms.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        lane_decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        lane_decision.reason,
        lane_decision.hint,
        event.event_id
    );

    if blocked_by_resource || blocked_by_ownership || blocked_by_continuation {
        return Err(AppError::blocked(format!("team dispatch 차단\n{}", report)));
    }

    Ok(report)
}
