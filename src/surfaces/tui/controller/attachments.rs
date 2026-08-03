use super::super::view_model::InteractiveState;
use super::TuiRuntimePort;

pub(super) fn capture_attachment_notice(
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    path: &str,
) -> String {
    match runtime.capture_attachment(path) {
        Ok(attachment) => {
            let notice = format!(
                "첨부됨 · {} · {} bytes\n다음 요청에 포함됩니다.",
                attachment.display_name, attachment.size_bytes
            );
            state.add_attachment(attachment);
            notice
        }
        Err(error) => error.message,
    }
}

pub(super) fn capture_clipboard_image_notice(
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
) -> String {
    match runtime.capture_clipboard_image() {
        Ok(attachment) => {
            let notice = format!(
                "클립보드 이미지 첨부됨 · {} · {} bytes\n다음 요청에 포함됩니다.",
                attachment.display_name, attachment.size_bytes
            );
            state.add_attachment(attachment);
            notice
        }
        Err(error) => error.message,
    }
}

pub(super) fn attachment_path_candidate(value: &str) -> Option<String> {
    let value = strip_bracketed_paste_markers(trim_clipboard_boundaries(value));
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value);
    let value = strip_bracketed_paste_markers(trim_clipboard_boundaries(value));
    let decoded = value.replace("\\ ", " ");
    let value = decoded.strip_prefix("file://").unwrap_or(&decoded);
    let value = trim_clipboard_boundaries(value);
    let path_like = value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/");
    let extension = std::path::Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    (path_like
        && matches!(
            extension.as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "rs"
                | "toml"
                | "md"
                | "txt"
                | "json"
                | "yaml"
                | "yml"
                | "py"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "go"
                | "java"
                | "kt"
                | "kts"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "hpp"
                | "sh"
                | "zsh"
                | "fish"
                | "html"
                | "css"
                | "scss"
                | "sql"
                | "xml"
                | "csv"
                | "log"
        ))
    .then(|| value.to_string())
}

fn strip_bracketed_paste_markers(value: &str) -> &str {
    let value = value.strip_prefix("\u{001b}[200~").unwrap_or(value);
    value.strip_suffix("\u{001b}[201~").unwrap_or(value)
}

fn trim_clipboard_boundaries(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
            )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_path_candidate_strips_invisible_boundary_characters() {
        let path = "/var/folders/example/clipboard-image.png\u{2060}";

        assert_eq!(
            attachment_path_candidate(path).as_deref(),
            Some("/var/folders/example/clipboard-image.png")
        );
    }

    #[test]
    fn clipboard_file_url_and_escaped_spaces_remain_attachable() {
        let path = "file:///private/tmp/My\\ Screenshot.png";

        assert_eq!(
            attachment_path_candidate(path).as_deref(),
            Some("/private/tmp/My Screenshot.png")
        );
    }

    #[test]
    fn leaked_bracketed_paste_markers_do_not_turn_an_image_path_into_a_command() {
        let path = "\u{001b}[200~/var/folders/example/clipboard-image.png\u{001b}[201~";

        assert_eq!(
            attachment_path_candidate(path).as_deref(),
            Some("/var/folders/example/clipboard-image.png")
        );
    }
}
