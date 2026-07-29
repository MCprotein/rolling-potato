use super::admission::{
    classify_policy_inputs, normalize_ownership_claims, record_approval_request,
};
use super::report_format::*;
use super::*;

pub fn admission_report(
    requested_lanes: u32,
    write_paths: &[String],
    owned_write_paths: &[(u32, String)],
    commands: &[String],
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let decision = resource::team_lane_decision(pressure, requested_lanes);
    let policy_write_paths = policy_write_paths(write_paths, owned_write_paths);
    let policy_gate = evaluate_policy_gate(classify_policy_inputs(&policy_write_paths, commands)?);
    let ownership_gate = evaluate_ownership_gate(
        decision.admitted_lanes,
        normalize_ownership_claims(owned_write_paths)?,
    );
    let blocked_by_resource = decision.is_blocked();
    let blocked_by_policy = policy_gate.is_blocked();
    let blocked_by_ownership = ownership_gate.is_blocked();
    let dispatch_blocked = if blocked_by_resource || blocked_by_policy || blocked_by_ownership {
        "yes"
    } else {
        "no"
    };
    let event_type =
        admission_event_type(decision.admission, blocked_by_policy, blocked_by_ownership);
    let event = ledger::new_event_for(
        &identity,
        event_type,
        admission_summary(decision.admission, blocked_by_policy, blocked_by_ownership),
        &format!(
            "requested_lanes={} admitted_lanes={} admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} policy_status={} policy_blocked={} ownership_status={} ownership_blocked={} write_paths={} owned_write_paths={} commands={} reason={}",
            decision.requested_lanes,
            decision.admitted_lanes,
            decision.admission.as_str(),
            dispatch_blocked,
            decision.fallback,
            decision.pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            policy_gate.status,
            policy_gate.blocked_label(),
            ownership_gate.status,
            ownership_gate.blocked_label(),
            display_list(write_paths),
            display_owned_write_paths(owned_write_paths),
            display_redacted_list(commands),
            decision.reason
        ),
    );
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)?;
    let approval_request = record_approval_request(
        &identity,
        &event,
        overall_status(decision.admission, blocked_by_policy, blocked_by_ownership),
        &policy_gate,
        &ownership_gate,
    )?;

    let report = format!(
        "team admission\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- policy checks: {}\n- policy status: {}\n- policy blocked: {}\n- write paths: {}\n- commands: {}\n- policy decisions:\n{}\n- ownership claims: {}\n- ownership status: {}\n- ownership blocked: {}\n- owned write paths: {}\n- ownership decisions:\n{}\n- approval request: {}\n- approval request path: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: admission gate only; records the decision and does not start workers, mutate team stages, bypass approval policy, or write files.",
        overall_status(decision.admission, blocked_by_policy, blocked_by_ownership),
        store.path.display(),
        identity.session_id,
        decision.requested_lanes,
        decision.admitted_lanes,
        decision.admission.as_str(),
        dispatch_blocked,
        decision.fallback,
        policy_gate.checks.len(),
        policy_gate.status,
        policy_gate.blocked_label(),
        display_list(write_paths),
        display_redacted_list(commands),
        format_policy_checks(&policy_gate.checks),
        ownership_gate.checks.len(),
        ownership_gate.status,
        ownership_gate.blocked_label(),
        display_owned_write_paths(owned_write_paths),
        format_ownership_checks(&ownership_gate.checks),
        approval_request
            .as_ref()
            .map(|request| request.request_id.as_str())
            .unwrap_or("not-required"),
        approval_request
            .as_ref()
            .map(|request| request.path.display().to_string())
            .unwrap_or_else(|| "없음".to_string()),
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
        decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        decision.reason,
        decision.hint,
        event.event_id
    );

    if blocked_by_resource || blocked_by_policy || blocked_by_ownership {
        return Err(AppError::blocked(format!(
            "team admission 차단\n{}",
            report
        )));
    }

    Ok(report)
}
