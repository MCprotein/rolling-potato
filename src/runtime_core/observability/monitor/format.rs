pub(super) fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_str(value: Option<&str>) -> String {
    value.unwrap_or("없음").to_string()
}

pub(super) fn ms_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}ms"))
        .unwrap_or_else(|| "미기록".to_string())
}

pub(super) fn tps_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} tok/s"))
        .unwrap_or_else(|| "미기록".to_string())
}

pub(super) fn score_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}/3"))
        .unwrap_or_else(|| "미기록".to_string())
}
