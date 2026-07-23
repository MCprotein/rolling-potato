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

pub(super) fn looks_like_attachment_path(value: &str) -> bool {
    let value = value.trim().trim_matches(['"', '\'']);
    let path_like = value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with("~/")
        || value.starts_with("file://");
    let extension = std::path::Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    path_like
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
        )
}
