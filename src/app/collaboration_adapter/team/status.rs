use super::report_format::*;
use super::*;

pub fn status_report() -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::status()?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let decision = resource::team_lane_decision(pressure, resource::DEFAULT_TEAM_REQUESTED_LANES);
    let dispatch_blocked = if decision.is_blocked() { "yes" } else { "no" };
    let latest_team_event = latest_team_runtime_event(&identity)?;
    let active_parent = state::active_workflow_id()?;
    let latest_team_state = match active_parent.as_deref() {
        Some(parent_workflow_id) => team_state::latest_for_parent(parent_workflow_id)?,
        None => None,
    };

    Ok(format!(
        "team status\n- status: admission-preview\n- observability store: {}\n- resource samples: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- current team id: {}\n- current team stage: {}\n- current team status: {}\n- current team revision: {}\n- current team execution mode: {}\n- latest team runtime event: {}\n- latest team runtime summary: {}\n- latest team runtime event id: {}\n- reason: {}\n- hint: {}\n- boundary: read-only status only; does not start subagents, dispatch team lanes, mutate workflows, or bypass approval/file ownership policy.",
        store.path.display(),
        store.resource_samples,
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
        decision.requested_lanes,
        decision.admitted_lanes,
        decision.admission.as_str(),
        dispatch_blocked,
        decision.fallback,
        latest_team_state
            .as_ref()
            .map(|record| record.team_id.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.stage.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.status.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.revision.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        latest_team_state
            .as_ref()
            .map(|record| record.execution_mode.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.event_type.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.summary.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.event_id.as_str())
            .unwrap_or("없음"),
        decision.reason,
        decision.hint
    ))
}
