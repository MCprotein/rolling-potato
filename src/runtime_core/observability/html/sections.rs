use std::fmt::Write;

use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, ResourceSampleMetric, StoreStatus,
};

use super::text::{optional_f64, optional_u64, policy_class, pressure_class, safe_html_text};
use super::ReportData;

pub(super) fn render_store_summary(html: &mut String, data: &ReportData<StoreStatus>) {
    html.push_str(
        "<section aria-labelledby=\"summary-title\"><h2 id=\"summary-title\">현재 요약</h2>",
    );
    match data {
        ReportData::Available(store) => {
            write!(
                html,
                "<div class=\"summary\">{}{}{}{}{}{}\
                 </div><p class=\"muted\">schema migration v{} · ledger events {}</p>",
                metric("session", store.sessions),
                metric("workflow", store.workflows),
                metric("model run", store.model_runs),
                metric("token record", store.token_records),
                metric("resource sample", store.resource_samples),
                metric("stop gate", store.stop_gate_results),
                store.migration_version,
                store.ledger_events
            )
            .expect("writing to String cannot fail");
        }
        ReportData::Unavailable => {
            html.push_str("<p class=\"empty\">observability store 상태를 읽지 못했습니다. 다른 section은 그대로 표시합니다.</p>");
        }
    }
    html.push_str("</section>\n");
}

pub(super) fn render_resource(html: &mut String, data: &ReportData<Option<ResourceSampleMetric>>) {
    html.push_str(
        "<section aria-labelledby=\"resource-title\"><h2 id=\"resource-title\">최신 resource 상태</h2>",
    );
    match data {
        ReportData::Available(Some(sample)) => {
            let pressure = safe_html_text(&sample.pressure_status);
            write!(
                html,
                "<p class=\"status {}\">상태: {}</p><dl>\
                 <dt>metric timestamp</dt><dd>{} ms (Unix epoch)</dd>\
                 <dt>CPU</dt><dd>{}</dd>\
                 <dt>average RSS bytes</dt><dd>{}</dd>\
                 <dt>peak RSS bytes</dt><dd>{}</dd>\
                 <dt>disk bytes</dt><dd>{}</dd>\
                 <dt>sample count</dt><dd>{}</dd></dl>",
                pressure_class(&sample.pressure_status),
                pressure,
                sample.recorded_at_ms,
                optional_f64(sample.process_cpu_percent, "%"),
                optional_u64(sample.average_rss_bytes),
                optional_u64(sample.peak_rss_bytes),
                optional_u64(sample.disk_bytes),
                sample.sample_count
            )
            .expect("writing to String cannot fail");
        }
        ReportData::Available(None) => {
            html.push_str("<p class=\"empty\">아직 resource sample이 없습니다. 다음 model run 이후 다시 export하세요.</p>");
        }
        ReportData::Unavailable => {
            html.push_str("<p class=\"empty\">resource metric을 읽지 못했습니다. 다른 section은 그대로 표시합니다.</p>");
        }
    }
    html.push_str("</section>\n");
}

pub(super) fn render_models(
    html: &mut String,
    data: &ReportData<Vec<ModelMetricSummary>>,
    candidate_summary: &str,
) {
    html.push_str(
        "<section aria-labelledby=\"models-title\"><h2 id=\"models-title\">모델별 metric</h2>",
    );
    match data {
        ReportData::Available(rows) if rows.is_empty() => {
            write!(
                html,
                "<p class=\"empty\">기록된 model run이 없습니다. 현재 candidate: {}</p>",
                safe_html_text(candidate_summary)
            )
            .expect("writing to String cannot fail");
        }
        ReportData::Available(rows) => {
            html.push_str(
                "<div class=\"table-wrap\"><table><caption>기록된 모델별 token과 latency</caption>\
                 <thead><tr><th>model</th><th>runs</th><th>prompt</th><th>completion</th>\
                 <th>total</th><th>avg latency</th><th>avg tok/s</th></tr></thead><tbody>",
            );
            for row in rows {
                write!(
                    html,
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
                     <td>{}</td><td>{}</td></tr>",
                    safe_html_text(&row.model_id),
                    row.runs,
                    row.prompt_tokens,
                    row.completion_tokens,
                    row.total_tokens,
                    optional_f64(row.avg_latency_ms, " ms"),
                    optional_f64(row.avg_tokens_per_second, " tok/s")
                )
                .expect("writing to String cannot fail");
            }
            html.push_str("</tbody></table></div>");
        }
        ReportData::Unavailable => {
            html.push_str("<p class=\"empty\">model metric을 읽지 못했습니다. 다른 section은 그대로 표시합니다.</p>");
        }
    }
    html.push_str("</section>\n");
}

pub(super) fn render_performance(html: &mut String, data: &ReportData<OptimizationPolicy>) {
    html.push_str("<section aria-labelledby=\"performance-title\"><h2 id=\"performance-title\">성능과 optimization policy</h2>");
    let ReportData::Available(policy) = data else {
        html.push_str("<p class=\"empty\">performance/optimization policy를 읽지 못했습니다. 다른 section은 그대로 표시합니다.</p></section>\n");
        return;
    };
    let decision = &policy.decision;
    let evidence = &policy.benchmark_evidence;
    write!(
        html,
        "<p class=\"status {}\">policy status: {}</p><dl>\
         <dt>latest pressure</dt><dd>{}</dd>\
         <dt>p95 latency</dt><dd>{}</dd>\
         <dt>average throughput</dt><dd>{}</dd>\
         <dt>peak RSS bytes</dt><dd>{}</dd>\
         <dt>context clamp</dt><dd>{}회 / {} tokens dropped</dd>\
         <dt>recommended context</dt><dd>{}</dd>\
         <dt>recommended team lanes</dt><dd>{}</dd>\
         <dt>fallback</dt><dd>{}</dd>\
         <dt>model route hint</dt><dd>{}</dd>\
         <dt>reason</dt><dd>{}</dd>\
         <dt>next hint</dt><dd>{}</dd>\
         <dt>measured benchmark</dt><dd>{} runs · pass {} · fail {} · avg score {}</dd>\
         <dt>latest benchmark</dt><dd>{} / {}</dd></dl></section>",
        policy_class(decision.status.as_str()),
        safe_html_text(decision.status.as_str()),
        safe_html_text(&policy.latest_resource_pressure),
        optional_f64(policy.p95_latency_ms, " ms"),
        optional_f64(policy.avg_tokens_per_second, " tok/s"),
        optional_u64(policy.peak_rss_bytes),
        policy.context_clamp_count,
        policy.context_tokens_dropped,
        decision
            .recommended_context_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미기록".to_owned()),
        decision.recommended_lanes,
        safe_html_text(decision.fallback),
        safe_html_text(decision.model_hint.as_str()),
        safe_html_text(decision.reason),
        safe_html_text(decision.hint),
        evidence.measured_runs,
        evidence.passed_runs,
        evidence.failed_runs,
        evidence
            .avg_score
            .map(|value| format!("{value:.2}/3"))
            .unwrap_or_else(|| "미기록".to_owned()),
        safe_html_text(evidence.latest_model_id.as_deref().unwrap_or("미기록")),
        safe_html_text(
            evidence
                .latest_benchmark_name
                .as_deref()
                .unwrap_or("미기록")
        )
    )
    .expect("writing to String cannot fail");
}

pub(super) fn render_privacy(html: &mut String) {
    html.push_str(
        "<section aria-labelledby=\"privacy-title\"><h2 id=\"privacy-title\">privacy 경계</h2>\
         <dl><dt>raw prompt/source</dt><dd>저장·표시 안 함</dd>\
         <dt>credential</dt><dd>표시 안 함</dd>\
         <dt>local filesystem path</dt><dd>redacted</dd>\
         <dt>network</dt><dd>요청 없음</dd></dl></section>\n",
    );
}

fn metric(label: &str, value: i64) -> String {
    format!(
        "<div class=\"metric\"><span>{}</span><strong>{value}</strong></div>",
        safe_html_text(label)
    )
}
