use crate::adapters::filesystem::config;

pub fn report() -> String {
    let sources = config::discover();
    format!(
        "config 경계\n- config dir: {}\n- config file: {}\n- source: user > project > default 순서로 해석 예정\n- 현재 상태: config read/write는 Phase 2 state API 이후 활성화",
        sources.directory.display(),
        sources.file.display()
    )
}
