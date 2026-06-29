use crate::{model, paths};

pub fn status_report() -> String {
    format!(
        "monitor 상태\n- observability store: {}\n- runtime ledger: {}\n- raw prompt/source 저장: 기본 비활성\n- 현재 상태: SQLite projection은 Phase 2에서 생성 예정",
        paths::observability_db_file().display(),
        paths::runtime_ledger_file().display()
    )
}

pub fn models_report() -> String {
    format!(
        "model monitoring\n- model candidates: {}\n- token/latency/resource metric: Phase 2 observability store 이후 기록\n- export: JSONL/CSV 예정",
        model::candidate_summary()
    )
}
