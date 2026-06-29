use crate::{backend, cache, model, paths};

pub fn init_report() -> String {
    format!(
        "rpotato init 계획\n- app data root: {}\n- config file: {}\n- state dir: {}\n- project state dir: {}\n- backend: {}\n- model: {}\n- 동작: 현재는 경로와 정책만 보고하며 다운로드/파일 생성은 수행하지 않습니다.",
        paths::app_data_root().display(),
        paths::config_file().display(),
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        backend::doctor_summary(),
        model::candidate_summary()
    )
}

pub fn doctor_report() -> String {
    let backend = backend::doctor_summary();
    let cache = cache::status_summary();
    let models = model::candidate_summary();

    format!(
        "rpotato 진단\n- CLI: 사용 가능\n- runtime core: CLI surface에서 분리된 report boundary 사용\n- backend: {}\n- model: {}\n- cache: {}",
        backend, models, cache
    )
}
