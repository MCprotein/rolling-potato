#[cfg(unix)]
#[test]
fn unix_pid_arg_rejects_wrapping_values() {
    assert_eq!(backend_process::unix_pid_arg(0), None);
    assert_eq!(backend_process::unix_pid_arg(u32::MAX), None);
    assert_eq!(
        backend_process::unix_pid_arg(i32::MAX as u32),
        Some((i32::MAX as u32).to_string())
    );
}

#[test]
fn health_check_report_is_diagnostic_not_process_start() {
    let report = health_check_report();
    assert!(report.contains("backend health check"));
    assert!(report.contains("health URL"));
    assert!(report.contains("timeout ms"));
}

#[test]
fn model_id_comes_from_model_file_stem() {
    let model_id = model_id_from_path(Path::new("/tmp/Qwen3.5-4B-Q4_K_M.gguf"));

    assert_eq!(model_id, "Qwen3.5-4B-Q4_K_M");
}
