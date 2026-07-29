use super::super::render::{push_footer, push_header, push_kv, push_rule, push_section};
use super::super::view_model::EvidenceReportView;

pub(crate) fn render_evidence_report(width: usize, view: &EvidenceReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - evidence");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(&mut lines, width, "session", &view.session_id);
    push_kv(&mut lines, width, "mode", "read-only evidence status");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stores");
    push_kv(
        &mut lines,
        width,
        "runtime evidence",
        &view.runtime_evidence_file,
    );
    push_kv(
        &mut lines,
        width,
        "runtime records",
        &view.runtime_evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "project evidence",
        &view.project_evidence_dir,
    );
    push_kv(
        &mut lines,
        width,
        "project artifacts",
        &view.project_artifacts.to_string(),
    );
    push_kv(&mut lines, width, "observability", &view.observability_path);
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stop gate boundary");
    push_kv(
        &mut lines,
        width,
        "recorded evidence",
        &view.evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "stop gate results",
        &view.stop_gate_results.to_string(),
    );
    push_kv(&mut lines, width, "stale policy", &view.stale_policy);
    push_kv(
        &mut lines,
        width,
        "terminal gate",
        "not implemented; this view does not pass or fail workflows",
    );
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "validate",
        "rpotato evidence validate <artifact-pointer>",
    );
    push_kv(
        &mut lines,
        width,
        "raw prompt/source",
        "disabled by default",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}
