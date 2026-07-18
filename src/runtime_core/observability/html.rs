//! Self-contained, local-only HTML rendering for monitor snapshots.

use std::fmt::Write;

use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, ResourceSampleMetric, StoreStatus,
};

pub(crate) enum ReportData<T> {
    Available(T),
    Unavailable,
}

pub(crate) struct HtmlReportSnapshot {
    pub generated_at_ms: u128,
    pub store: StoreStatus,
    pub latest_resource: ReportData<Option<ResourceSampleMetric>>,
    pub model_summaries: ReportData<Vec<ModelMetricSummary>>,
    pub model_candidate_summary: String,
    pub optimization_policy: OptimizationPolicy,
}

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; \
    img-src 'none'; script-src 'none'; connect-src 'none'; font-src 'none'; \
    object-src 'none'; media-src 'none'; frame-src 'none'; worker-src 'none'; \
    manifest-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #f4f1e8;
  --panel: #fffdf7;
  --text: #20221f;
  --muted: #62685f;
  --line: #c8c9bd;
  --accent: #235d48;
  --healthy: #17633d;
  --warning: #7a5200;
  --failed: #a02b2b;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
header, main, footer { width: min(72rem, calc(100% - 2rem)); margin-inline: auto; }
header { padding: 2.5rem 0 1.25rem; border-bottom: 2px solid var(--text); }
h1 { margin: 0 0 .4rem; font-size: clamp(1.7rem, 4vw, 2.8rem); letter-spacing: -.04em; }
h2 { margin: 0 0 .8rem; font-size: 1.15rem; }
p { margin: .35rem 0; }
.eyebrow { color: var(--accent); font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
.muted, footer { color: var(--muted); }
main { display: grid; gap: 1rem; padding: 1rem 0 2rem; }
section { padding: 1rem; background: var(--panel); border: 1px solid var(--line); }
.summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .65rem; }
.metric { min-width: 0; padding: .8rem; border-left: 3px solid var(--accent); background: var(--bg); }
.metric strong { display: block; margin-top: .25rem; font-size: 1.15rem; overflow-wrap: anywhere; }
.status { font-weight: 700; }
.healthy { color: var(--healthy); }
.warning { color: var(--warning); }
.failed { color: var(--failed); }
.table-wrap { max-width: 100%; overflow-x: auto; }
table { width: 100%; border-collapse: collapse; white-space: nowrap; }
caption { padding: 0 0 .6rem; color: var(--muted); text-align: left; }
th, td { padding: .55rem .65rem; border-bottom: 1px solid var(--line); text-align: left; }
th { color: var(--muted); font-size: .85rem; }
.empty { padding: .8rem; border: 1px dashed var(--line); color: var(--muted); }
dl { display: grid; grid-template-columns: minmax(10rem, .4fr) 1fr; margin: 0; }
dt, dd { margin: 0; padding: .45rem 0; border-bottom: 1px solid var(--line); }
dt { color: var(--muted); }
dd { overflow-wrap: anywhere; }
footer { padding: 0 0 2rem; }
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #171a18;
    --panel: #202421;
    --text: #eceee9;
    --muted: #adb5aa;
    --line: #454d47;
    --accent: #70c69e;
    --healthy: #70c69e;
    --warning: #e1bb68;
    --failed: #f18a8a;
  }
}
@media (max-width: 48rem) {
  header, main, footer { width: min(100% - 1rem, 72rem); }
  header { padding-top: 1.5rem; }
  .summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  dl { grid-template-columns: 1fr; }
  dt { padding-bottom: 0; border-bottom: 0; }
  dd { padding-top: .15rem; }
}
@media (max-width: 30rem) {
  .summary { grid-template-columns: 1fr; }
  section { padding: .8rem; }
}
"#;

pub(crate) fn render_report(snapshot: &HtmlReportSnapshot) -> String {
    let policy = &snapshot.optimization_policy;
    let mut html = String::with_capacity(12_000);

    html.push_str("<!doctype html>\n<html lang=\"ko\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    write!(
        html,
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">\n",
        CONTENT_SECURITY_POLICY
    )
    .expect("writing to String cannot fail");
    html.push_str("<title>rolling-potato monitor report</title>\n<style>");
    html.push_str(STYLE);
    html.push_str("</style>\n</head>\n<body>\n");
    write!(
        html,
        "<header><p class=\"eyebrow\">local monitor snapshot</p>\
         <h1>rolling-potato monitor report</h1>\
         <p>로컬 데이터만 읽어 만든 정적 report입니다.</p>\
         <p class=\"muted\">생성 시각: {} ms (Unix epoch) · data source: SQLite projection + canonical ledger</p>\
         </header>\n<main>\n",
        snapshot.generated_at_ms
    )
    .expect("writing to String cannot fail");

    render_store_summary(&mut html, &snapshot.store);
    render_resource(&mut html, &snapshot.latest_resource);
    render_models(
        &mut html,
        &snapshot.model_summaries,
        &snapshot.model_candidate_summary,
    );
    render_performance(&mut html, policy);
    render_privacy(&mut html);

    write!(
        html,
        "</main>\n<footer>rpotato {} · read-only · offline · redacted</footer>\n</body>\n</html>\n",
        env!("CARGO_PKG_VERSION")
    )
    .expect("writing to String cannot fail");
    html
}

fn render_store_summary(html: &mut String, store: &StoreStatus) {
    write!(
        html,
        "<section aria-labelledby=\"summary-title\"><h2 id=\"summary-title\">현재 요약</h2>\
         <div class=\"summary\">{}{}{}{}{}{}\
         </div><p class=\"muted\">schema migration v{} · ledger events {}</p></section>\n",
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

fn render_resource(html: &mut String, data: &ReportData<Option<ResourceSampleMetric>>) {
    html.push_str(
        "<section aria-labelledby=\"resource-title\"><h2 id=\"resource-title\">최신 resource 상태</h2>",
    );
    match data {
        ReportData::Available(Some(sample)) => {
            let pressure = escape_html(&sample.pressure_status);
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

fn render_models(
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
                escape_html(candidate_summary)
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
                    escape_html(&row.model_id),
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

fn render_performance(html: &mut String, policy: &OptimizationPolicy) {
    let decision = &policy.decision;
    let evidence = &policy.benchmark_evidence;
    write!(
        html,
        "<section aria-labelledby=\"performance-title\"><h2 id=\"performance-title\">성능과 optimization policy</h2>\
         <p class=\"status {}\">policy status: {}</p><dl>\
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
         <dt>latest benchmark</dt><dd>{} / {}</dd></dl></section>\n",
        policy_class(decision.status.as_str()),
        escape_html(decision.status.as_str()),
        escape_html(&policy.latest_resource_pressure),
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
        escape_html(decision.fallback),
        escape_html(decision.model_hint.as_str()),
        escape_html(decision.reason),
        escape_html(decision.hint),
        evidence.measured_runs,
        evidence.passed_runs,
        evidence.failed_runs,
        evidence
            .avg_score
            .map(|value| format!("{value:.2}/3"))
            .unwrap_or_else(|| "미기록".to_owned()),
        escape_html(evidence.latest_model_id.as_deref().unwrap_or("미기록")),
        escape_html(
            evidence
                .latest_benchmark_name
                .as_deref()
                .unwrap_or("미기록")
        )
    )
    .expect("writing to String cannot fail");
}

fn render_privacy(html: &mut String) {
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
        escape_html(label)
    )
}

fn optional_f64(value: Option<f64>, suffix: &str) -> String {
    value
        .filter(|value| value.is_finite())
        .map(|value| format!("{value:.1}{suffix}"))
        .unwrap_or_else(|| "미기록".to_owned())
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "미기록".to_owned())
}

fn pressure_class(value: &str) -> &'static str {
    match value {
        "normal" => "healthy",
        "degraded" => "warning",
        "critical" => "failed",
        _ => "muted",
    }
}

fn policy_class(value: &str) -> &'static str {
    match value {
        "recommend" => "healthy",
        "constrained" | "insufficient-evidence" => "warning",
        "blocked" => "failed",
        _ => "muted",
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::runtime_core::inference::resource::{
        ModelRouteHint, OptimizationPolicyDecision, OptimizationPolicyStatus,
    };
    use crate::runtime_core::observability::facade::BenchmarkEvidenceSummary;

    use super::*;

    #[test]
    fn report_is_self_contained_responsive_and_escapes_dynamic_values() {
        let mut snapshot = snapshot();
        snapshot.store.path = PathBuf::from("/secret/private/observability.sqlite");
        snapshot.store.recovered_from = Some(PathBuf::from("/secret/private/recovered.sqlite"));
        snapshot.model_summaries = ReportData::Available(vec![ModelMetricSummary {
            model_id: "<script>alert('x')</script>&model".to_owned(),
            runs: 1,
            prompt_tokens: 2,
            completion_tokens: 3,
            total_tokens: 5,
            avg_latency_ms: Some(6.0),
            avg_tokens_per_second: Some(7.0),
        }]);

        let report = render_report(&snapshot);

        assert!(report.starts_with("<!doctype html>"));
        assert!(report.contains("Content-Security-Policy"));
        assert!(report.contains("default-src 'none'"));
        assert!(report.contains("@media (max-width: 48rem)"));
        assert!(report.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;model"));
        assert!(!report.contains("<script>"));
        assert!(!report.contains("https://"));
        assert!(!report.contains("http://"));
        assert!(!report.contains("/secret/private"));
        assert!(report.contains("<main>"));
        assert!(report.contains("<caption>"));
        assert!(report.contains("raw prompt/source</dt><dd>저장·표시 안 함"));
    }

    #[test]
    fn empty_and_unavailable_states_preserve_the_document() {
        let mut snapshot = snapshot();
        snapshot.latest_resource = ReportData::Unavailable;
        snapshot.model_summaries = ReportData::Available(Vec::new());
        snapshot.model_candidate_summary = "후보 <A>".to_owned();

        let report = render_report(&snapshot);

        assert!(report.contains("resource metric을 읽지 못했습니다"));
        assert!(report.contains("기록된 model run이 없습니다"));
        assert!(report.contains("후보 &lt;A&gt;"));
        assert!(report.ends_with("</html>\n"));
    }

    fn snapshot() -> HtmlReportSnapshot {
        let store = StoreStatus {
            path: PathBuf::from("/state/observability.sqlite"),
            recovered_from: None,
            migration_version: 6,
            ledger_events: 11,
            sessions: 2,
            workflows: 3,
            transcript_records: 4,
            model_runs: 5,
            token_records: 6,
            resource_samples: 7,
            benchmark_runs: 8,
            evidence_records: 9,
            stop_gate_results: 10,
        };
        HtmlReportSnapshot {
            generated_at_ms: 123,
            store: store.clone(),
            latest_resource: ReportData::Available(None),
            model_summaries: ReportData::Available(Vec::new()),
            model_candidate_summary: "candidate-a".to_owned(),
            optimization_policy: OptimizationPolicy {
                store,
                model_runs: 5,
                resource_samples: 7,
                latest_resource_pressure: "normal".to_owned(),
                context_clamp_count: 1,
                context_tokens_dropped: 12,
                p95_latency_ms: Some(42.0),
                avg_tokens_per_second: Some(8.0),
                peak_rss_bytes: Some(1024),
                benchmark_evidence: BenchmarkEvidenceSummary {
                    measured_runs: 2,
                    passed_runs: 1,
                    failed_runs: 1,
                    avg_score: Some(2.5),
                    latest_benchmark_run_id: Some("benchmark-1".to_owned()),
                    latest_model_id: Some("model-a".to_owned()),
                    latest_benchmark_name: Some("local-smoke".to_owned()),
                },
                decision: OptimizationPolicyDecision {
                    status: OptimizationPolicyStatus::Recommend,
                    recommended_context_tokens: Some(2048),
                    recommended_lanes: 2,
                    fallback: "sequential",
                    model_hint: ModelRouteHint::Keep,
                    reason: "local evidence",
                    hint: "keep measuring",
                },
            },
        }
    }
}
