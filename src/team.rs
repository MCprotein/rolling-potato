use crate::app::AppError;
use crate::{observability, resource};

pub fn status_report() -> Result<String, AppError> {
    let store = observability::status()?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let decision = resource::team_lane_decision(pressure, resource::DEFAULT_TEAM_REQUESTED_LANES);
    let dispatch_blocked = if decision.is_blocked() { "yes" } else { "no" };

    Ok(format!(
        "team status\n- status: admission-preview\n- observability store: {}\n- resource samples: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- reason: {}\n- hint: {}\n- boundary: read-only status only; does not start subagents, dispatch team lanes, mutate workflows, or bypass approval/file ownership policy.",
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
        decision.reason,
        decision.hint
    ))
}

fn pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn status_falls_back_to_sequential_when_resource_sample_is_missing() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-status-no-sample-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = status_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("team status"));
        assert!(report.contains("resource sample source: no-sample"));
        assert!(report.contains("resource pressure: unknown"));
        assert!(report.contains("admission: sequential-fallback"));
        assert!(report.contains("admitted lanes: 1"));
        assert!(report.contains("boundary: read-only"));
    }

    #[test]
    fn status_blocks_team_lanes_on_critical_resource_sample() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-status-critical-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        observability::record_resource_sample(&observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-team-critical".to_string(),
            session_id: "session-team-critical".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: Some(98.0),
            average_rss_bytes: Some(14 * 1024 * 1024 * 1024),
            peak_rss_bytes: Some(14 * 1024 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: "critical".to_string(),
            recorded_at_ms: 1234,
        })
        .unwrap();

        let report = status_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("resource sample source: latest-resource-sample"));
        assert!(report.contains("resource pressure: critical"));
        assert!(report.contains("resource cpu percent: 98.0"));
        assert!(report.contains("admission: blocked"));
        assert!(report.contains("dispatch blocked: yes"));
        assert!(report.contains("admitted lanes: 0"));
        assert!(report.contains("fallback: wait"));
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }
}
