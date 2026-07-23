//! Local attachment capture and text-request composition for the interactive TUI.

use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiAttachmentKind};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_ATTACHMENTS: usize = 8;

pub(super) fn capture(path_input: &str, session_id: &str) -> Result<TuiAttachment, AppError> {
    let source = normalized_source_path(path_input)?;
    let metadata = fs::symlink_metadata(&source).map_err(|error| {
        AppError::usage(format!(
            "첨부 파일을 찾을 수 없습니다.\n- path: {}\n- 이유: {error}",
            source.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "첨부를 차단했습니다.\n- path: {}\n- 이유: 일반 파일만 첨부할 수 있으며 symlink는 허용하지 않습니다.",
            source.display()
        )));
    }
    let kind = attachment_kind(&source)?;
    let max_bytes = match kind {
        TuiAttachmentKind::Image => MAX_IMAGE_BYTES,
        TuiAttachmentKind::Text => MAX_TEXT_BYTES,
    };
    if metadata.len() == 0 || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "첨부를 차단했습니다.\n- path: {}\n- size: {} bytes\n- 허용 범위: 1..={max_bytes} bytes",
            source.display(),
            metadata.len()
        )));
    }
    validate_content(&source, kind)?;
    let sha256 = integrity::sha256_file(&source)?;
    let display_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment")
        .to_string();
    let capture_dir = paths::app_data_root()
        .join("attachments")
        .join(safe_leaf(session_id));
    fs::create_dir_all(&capture_dir).map_err(|error| {
        AppError::runtime(format!(
            "첨부 저장소를 만들지 못했습니다: {} ({error})",
            capture_dir.display()
        ))
    })?;
    let stored_path = capture_dir.join(format!("{}-{}", sha256, safe_leaf(&display_name)));
    match fs::symlink_metadata(&stored_path) {
        Ok(stored_metadata)
            if stored_metadata.file_type().is_symlink() || !stored_metadata.is_file() =>
        {
            return Err(AppError::blocked(format!(
                "첨부 저장 경로를 차단했습니다.\n- path: {}\n- 이유: 기존 대상은 일반 파일이어야 하며 symlink는 허용하지 않습니다.",
                stored_path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::copy(&source, &stored_path).map_err(|error| {
                AppError::runtime(format!(
                    "첨부 파일을 app data에 캡처하지 못했습니다: {} ({error})",
                    stored_path.display()
                ))
            })?;
        }
        Err(error) => {
            return Err(AppError::runtime(format!(
                "첨부 저장 경로를 확인하지 못했습니다: {} ({error})",
                stored_path.display()
            )));
        }
    }
    if integrity::sha256_file(&stored_path)? != sha256 {
        let stored_metadata = fs::symlink_metadata(&stored_path).map_err(|error| {
            AppError::runtime(format!(
                "첨부 저장 경로를 다시 확인하지 못했습니다: {} ({error})",
                stored_path.display()
            ))
        })?;
        if stored_metadata.is_file() {
            let _ = fs::remove_file(&stored_path);
        }
        return Err(AppError::blocked(
            "첨부 캡처 후 SHA-256 검증에 실패했습니다.",
        ));
    }
    Ok(TuiAttachment {
        id: sha256,
        display_name,
        stored_path: stored_path.display().to_string(),
        size_bytes: metadata.len(),
        kind,
    })
}

pub(super) fn compose_request(
    prompt: &str,
    attachments: &[TuiAttachment],
) -> Result<String, AppError> {
    if attachments.len() > MAX_ATTACHMENTS {
        return Err(AppError::blocked(format!(
            "첨부는 요청당 최대 {MAX_ATTACHMENTS}개까지 사용할 수 있습니다."
        )));
    }
    if let Some(image) = attachments
        .iter()
        .find(|attachment| attachment.kind == TuiAttachmentKind::Image)
    {
        return Err(AppError::blocked(format!(
            "이미지 입력을 사용할 수 없습니다.\n- attachment: {}\n- 이유: 현재 검증된 모델/backend 구성은 text-only이며 mmproj가 없습니다.\n- 동작: 이미지를 모델에 보내지 않았습니다.\n- 다음: vision artifact와 mmproj가 검증된 모델 지원이 추가된 뒤 사용할 수 있습니다.",
            image.display_name
        )));
    }
    let mut request = prompt.trim().to_string();
    for attachment in attachments {
        let content = fs::read_to_string(&attachment.stored_path).map_err(|error| {
            AppError::blocked(format!(
                "텍스트 첨부를 읽지 못했습니다.\n- attachment: {}\n- 이유: {error}",
                attachment.display_name
            ))
        })?;
        request.push_str(&format!(
            "\n\n<attachment name=\"{}\">\n{}\n</attachment>",
            attachment.display_name, content
        ));
    }
    Ok(request)
}

fn normalized_source_path(value: &str) -> Result<PathBuf, AppError> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .replace("\\ ", " ");
    if value.trim().is_empty() {
        return Err(AppError::usage("첨부할 파일 경로가 필요합니다."));
    }
    if let Some(suffix) = value.strip_prefix("~/") {
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            AppError::usage("HOME을 확인할 수 없어 ~/ 경로를 해석하지 못했습니다.")
        })?;
        return Ok(home.join(suffix));
    }
    Ok(PathBuf::from(
        value.strip_prefix("file://").unwrap_or(&value),
    ))
}

fn attachment_kind(path: &Path) -> Result<TuiAttachmentKind, AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
        return Ok(TuiAttachmentKind::Image);
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

fn validate_content(path: &Path, kind: TuiAttachmentKind) -> Result<(), AppError> {
    match kind {
        TuiAttachmentKind::Text => fs::read_to_string(path)
            .map(|_| ())
            .map_err(|_| AppError::blocked("텍스트 첨부는 유효한 UTF-8 파일이어야 합니다.")),
        TuiAttachmentKind::Image => {
            let bytes = fs::read(path).map_err(|error| {
                AppError::runtime(format!("이미지 첨부를 읽지 못했습니다: {error}"))
            })?;
            let valid = bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                || bytes.starts_with(b"\xff\xd8\xff")
                || bytes.starts_with(b"GIF87a")
                || bytes.starts_with(b"GIF89a")
                || (bytes.starts_with(b"RIFF")
                    && bytes.get(8..12).is_some_and(|value| value == b"WEBP"));
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

fn safe_leaf(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .take(120)
        .collect::<String>();
    if value.is_empty() {
        "attachment".to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_text_into_app_data_and_composes_a_bounded_request() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-attachment-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        fs::write(root.join("source").join("sample.rs"), "fn main() {}\n").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(
            &root.join("source").join("sample.rs").display().to_string(),
            "session",
        )
        .unwrap();
        let request = compose_request("이 코드를 설명해줘", &[attachment.clone()]).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(Path::new(&attachment.stored_path).starts_with(root.join("data/attachments")));
        assert!(request.contains("<attachment name=\"sample.rs\">"));
        assert!(request.contains("fn main() {}"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn valid_image_is_captured_but_blocked_before_text_only_dispatch() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-image-attachment-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("screen.png"), b"\x89PNG\r\n\x1a\npayload").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment =
            capture(&root.join("screen.png").display().to_string(), "session").unwrap();
        let error = compose_request("이 이미지 봐줘", &[attachment]).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("text-only"));
        assert!(error.message.contains("모델에 보내지 않았습니다"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_preexisting_symlink_at_the_capture_target() {
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-attachment-target-symlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        let source = root.join("source").join("sample.rs");
        fs::write(&source, "fn main() {}\n").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let capture_dir = root.join("data/attachments/session");
        fs::create_dir_all(&capture_dir).unwrap();
        let sha256 = integrity::sha256_file(&source).unwrap();
        let outside = root.join("outside.rs");
        fs::write(&outside, "do not replace\n").unwrap();
        symlink(&outside, capture_dir.join(format!("{sha256}-sample.rs"))).unwrap();

        let error = capture(&source.display().to_string(), "session").unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("기존 대상은 일반 파일"));
        assert_eq!(fs::read_to_string(&outside).unwrap(), "do not replace\n");
        let _ = fs::remove_dir_all(root);
    }
}
