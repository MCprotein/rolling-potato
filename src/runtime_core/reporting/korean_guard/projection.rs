use super::classification::{
    classify_outside_text, is_safe_literal, runtime_field_value, strip_inline_code,
};

pub(super) fn stricter_projection(text: &str) -> String {
    let mut fenced = false;
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                fenced = !fenced;
                return Some(line.to_string());
            }
            if fenced {
                return Some(line.to_string());
            }
            project_outside_line(line)
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn project_outside_line(line: &str) -> Option<String> {
    let visible = strip_inline_code(line.trim());
    if projection_unit_is_safe(&visible) {
        return Some(line.to_string());
    }

    let mut projection = String::new();
    for unit in prose_units(line) {
        let visible = strip_inline_code(unit.trim());
        if projection_unit_is_safe(&visible) {
            if !projection.is_empty() && unit.starts_with(char::is_whitespace) {
                projection.push(' ');
            }
            projection.push_str(unit.trim());
        }
    }
    (!projection.is_empty()).then_some(projection)
}

fn projection_unit_is_safe(visible: &str) -> bool {
    let classification = classify_outside_text(visible);
    !classification.forbidden
        && (classification.has_hangul
            || classification.language_neutral
            || runtime_field_value(visible).is_some()
            || is_safe_literal(visible))
}

fn prose_units(line: &str) -> Vec<&str> {
    let mut units = Vec::new();
    let mut start = 0;
    for (index, character) in line.char_indices() {
        if matches!(character, '.' | '!' | '?' | '。' | '！' | '？') {
            let end = index + character.len_utf8();
            units.push(&line[start..end]);
            start = end;
        }
    }
    if start < line.len() {
        units.push(&line[start..]);
    }
    units
}
