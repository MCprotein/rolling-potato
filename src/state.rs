use crate::paths;

pub fn status_report() -> String {
    format!(
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- observability db: {}\n- 현재 active workflow: 없음\n- 현재 상태: state read/write API는 Phase 2에서 구현 예정",
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        paths::runtime_ledger_file().display(),
        paths::observability_db_file().display()
    )
}

pub fn cancel_report() -> String {
    format!(
        "cancel 결과\n- active workflow: 없음\n- ledger: {}\n- 동작: 취소할 실행이 없어 파일 변경 없이 종료했습니다.",
        paths::runtime_ledger_file().display()
    )
}
