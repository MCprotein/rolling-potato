//! Terminal-safe text sanitization, truncation, and display-cell measurement.

pub(crate) fn sanitize_terminal_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    let mut escaped = false;
                    for next in chars.by_ref() {
                        if next == '\u{0007}' || (escaped && next == '\\') {
                            break;
                        }
                        escaped = next == '\u{001b}';
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            out.push_str("<esc>");
        } else if ch.is_control() {
            match ch {
                '\n' => out.push_str("<lf>"),
                '\r' => out.push_str("<cr>"),
                '\t' => out.push_str("  "),
                _ => out.push_str("<ctl>"),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn truncate_chars(value: &str, width: usize) -> String {
    if display_cell_width(value) <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let available = width.saturating_sub(1);
    let mut used = 0;
    let mut out = String::new();
    for ch in value.chars() {
        let ch_width = terminal_cell_width(ch);
        if used + ch_width > available {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

pub(crate) fn display_cell_width(value: &str) -> usize {
    value.chars().map(terminal_cell_width).sum()
}

pub(super) fn wrap_terminal_text(value: &str, width: usize) -> Vec<String> {
    if value.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch_width = terminal_cell_width(ch);
        if !current.is_empty() && used + ch_width > width {
            lines.push(current);
            current = String::new();
            used = 0;
        }
        current.push(ch);
        used += ch_width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn terminal_cell_width(ch: char) -> usize {
    let code = ch as u32;
    if ch.is_control()
        || ch == '\u{200d}'
        || matches!(
            code,
            0x0300..=0x036f
                | 0x1ab0..=0x1aff
                | 0x1dc0..=0x1dff
                | 0x20d0..=0x20ff
                | 0xfe00..=0xfe0f
                | 0xfe20..=0xfe2f
                | 0xe0100..=0xe01ef
        )
    {
        return 0;
    }
    if matches!(
        code,
        0x1100..=0x115f
            | 0x2329..=0x232a
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f1e6..=0x1f1ff
            | 0x1f300..=0x1faff
            | 0x20000..=0x3fffd
    ) {
        2
    } else {
        1
    }
}
