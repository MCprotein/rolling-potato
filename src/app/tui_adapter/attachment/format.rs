use super::*;

pub(super) fn attachment_kind(path: &Path) -> Result<TuiAttachmentKind, AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
        return Ok(TuiAttachmentKind::Image);
    }
    if matches!(extension.as_str(), "gif" | "webp") {
        return Err(AppError::usage(
            "현재 이미지 첨부는 PNG와 JPEG 형식만 지원합니다.",
        ));
    }
    if matches!(
        extension.as_str(),
        "rs" | "toml"
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
    ) {
        return Ok(TuiAttachmentKind::Text);
    }
    Err(AppError::usage(format!(
        "지원하지 않는 첨부 형식입니다: {}",
        path.display()
    )))
}

pub(super) fn validate_content(path: &Path, kind: TuiAttachmentKind) -> Result<(), AppError> {
    match kind {
        TuiAttachmentKind::Text => fs::read_to_string(path)
            .map(|_| ())
            .map_err(|_| AppError::blocked("텍스트 첨부는 유효한 UTF-8 파일이어야 합니다.")),
        TuiAttachmentKind::Image => {
            let bytes = fs::read(path).map_err(|error| {
                AppError::runtime(format!("이미지 첨부를 읽지 못했습니다: {error}"))
            })?;
            let valid =
                bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.starts_with(b"\xff\xd8\xff");
            if valid {
                Ok(())
            } else {
                Err(AppError::blocked(
                    "이미지 확장자와 실제 파일 signature가 일치하지 않습니다.",
                ))
            }
        }
    }
}
