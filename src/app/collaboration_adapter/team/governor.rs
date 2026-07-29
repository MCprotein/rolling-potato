use super::report_format::*;
use super::*;

pub fn governor_report(
    requested_lanes: u32,
    requested_context_tokens: u32,
    context_limit_tokens: Option<u32>,
    model_tier: resource::ModelTier,
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let lane_decision = resource::team_lane_decision(pressure, requested_lanes);
    let context_limit_tokens = match context_limit_tokens {
        Some(limit) => limit,
        None => {
            crate::app::inference_adapter::context_window::effective_context_window()?.limit_tokens
        }
    };
    let context_decision = resource::context_model_governor_decision(
        pressure,
        requested_context_tokens,
        context_limit_tokens,
        model_tier,
    );
    let dispatch_blocked = if lane_decision.is_blocked() || context_decision.is_blocked() {
        "yes"
    } else {
        "no"
    };
    let status = governor_status(&context_decision, &lane_decision);
    let event = ledger::new_event_for(
        &identity,
        governor_event_type(status),
        governor_summary(status),
        &format!(
            "requested_lanes={} admitted_lanes={} lane_admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} requested_context_tokens={} context_limit_tokens={} effective_context_tokens={} context_action={} model_tier={} model_hint={} reason={}",
            lane_decision.requested_lanes,
            lane_decision.admitted_lanes,
            lane_decision.admission.as_str(),
            dispatch_blocked,
            lane_decision.fallback,
            pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            context_decision.requested_context_tokens,
            context_decision.context_limit_tokens,
            display_optional_u32(context_decision.effective_context_tokens),
            context_decision.context_action.as_str(),
            context_decision.model_tier.as_str(),
            context_decision.model_hint.as_str(),
            context_decision.reason
        ),
    );
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)?;

    let report = format!(
        "team governor\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- lane admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- requested context tokens: {}\n- context limit tokens: {}\n- effective context tokens: {}\n- context action: {}\n- model tier: {}\n- model route hint: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: governor preflight only; records context/model admission hints and does not start workers, select real model artifacts, mutate team stages, or execute tools.",
        status,
        store.path.display(),
        identity.session_id,
        lane_decision.requested_lanes,
        lane_decision.admitted_lanes,
        lane_decision.admission.as_str(),
        dispatch_blocked,
        lane_decision.fallback,
        context_decision.requested_context_tokens,
        context_decision.context_limit_tokens,
        display_optional_u32(context_decision.effective_context_tokens),
        context_decision.context_action.as_str(),
        context_decision.model_tier.as_str(),
        context_decision.model_hint.as_str(),
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
        context_decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        context_decision.reason,
        context_decision.hint,
        event.event_id
    );

    if lane_decision.is_blocked() || context_decision.is_blocked() {
        return Err(AppError::blocked(format!("team governor 차단\n{}", report)));
    }

    Ok(report)
}
