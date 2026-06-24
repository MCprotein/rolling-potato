use crate::paths;

pub fn doctor_summary() -> String {
    let path = paths::managed_backend_path();
    if path.exists() {
        format!("관리형 llama.cpp backend 발견 ({})", path.display())
    } else {
        "관리형 llama.cpp backend 미설치".to_string()
    }
}

pub fn doctor_report() -> String {
    let path = paths::managed_backend_path();
    let status = if path.exists() { "발견" } else { "미설치" };

    format!(
        "backend 진단\n- backend: llama.cpp sidecar\n- 관리형 binary: {}\n- path: {}\n- 다음 단계: 검증된 release URL과 checksum이 manifest에 들어오면 다운로드/설치를 활성화합니다.",
        status,
        path.display()
    )
}
