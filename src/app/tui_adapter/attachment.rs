//! Local attachment capture and text-request composition for the interactive TUI.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::runtime_core::inference::backend::{
    BackendChatImage, BackendChatInput, ResponseLanguage,
};
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiAttachmentKind};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_ATTACHMENTS: usize = 8;
const RESPONSE_RESERVE_TOKENS: usize = 512;
const RUNTIME_PROMPT_RESERVE_TOKENS: usize = 512;

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
    context_limit_tokens: Option<u32>,
) -> Result<BackendChatInput, AppError> {
    if attachments.len() > MAX_ATTACHMENTS {
        return Err(AppError::blocked(format!(
            "첨부는 요청당 최대 {MAX_ATTACHMENTS}개까지 사용할 수 있습니다."
        )));
    }
    let response_language = ResponseLanguage::from_user_request(prompt);
    let mut request = prompt.trim().to_string();
    let mut images = Vec::new();
    let text_budget = text_input_budget(attachments, context_limit_tokens)?;
    ensure_text_budget(&request, text_budget, None, context_limit_tokens)?;
    for attachment in attachments {
        match attachment.kind {
            TuiAttachmentKind::Text => {
                let content = verified_text(attachment)?;
                let rendered = format!(
                    "\n\n<attachment name=\"{}\">\n{}\n</attachment>",
                    safe_leaf(&attachment.display_name),
                    content
                );
                ensure_text_budget(
                    &format!("{request}{rendered}"),
                    text_budget,
                    Some(&attachment.display_name),
                    context_limit_tokens,
                )?;
                request.push_str(&rendered);
            }
            TuiAttachmentKind::Image => images.push(verified_image(attachment)?),
        }
    }
    Ok(BackendChatInput {
        text: request,
        images,
        response_language,
    })
}

fn text_input_budget(
    attachments: &[TuiAttachment],
    context_limit_tokens: Option<u32>,
) -> Result<Option<usize>, AppError> {
    if !attachments
        .iter()
        .any(|attachment| attachment.kind == TuiAttachmentKind::Text)
    {
        return Ok(None);
    }
    let limit = context_limit_tokens.ok_or_else(|| {
        AppError::blocked(
            "텍스트 첨부를 사용하려면 선택한 모델의 context length를 먼저 확인해야 합니다.",
        )
    })? as usize;
    let reserved = RESPONSE_RESERVE_TOKENS + RUNTIME_PROMPT_RESERVE_TOKENS;
    if limit <= reserved {
        return Err(AppError::blocked(format!(
            "선택한 모델의 context length가 텍스트 첨부를 처리하기에 너무 작습니다.\n- context: {limit} tokens\n- 응답·런타임 예약: {reserved} tokens"
        )));
    }
    Ok(Some(limit - reserved))
}

fn ensure_text_budget(
    text: &str,
    budget: Option<usize>,
    attachment: Option<&str>,
    context_limit_tokens: Option<u32>,
) -> Result<(), AppError> {
    let Some(budget) = budget else {
        return Ok(());
    };
    let estimated = crate::runtime_core::knowledge::compaction::estimate_tokens(text);
    if estimated <= budget {
        return Ok(());
    }
    let subject = attachment
        .map(|name| format!("텍스트 첨부 `{}`", safe_leaf(name)))
        .unwrap_or_else(|| "사용자 요청".to_string());
    Err(AppError::blocked(format!(
        "{subject}을(를) 현재 모델 context에 안전하게 넣을 수 없습니다.\n- 예상 입력: {estimated} tokens\n- 입력 예산: {budget} tokens\n- 모델 context: {} tokens\n- 동작: 첨부를 나누거나 더 긴 context를 지원하는 모델을 선택하세요.",
        context_limit_tokens.unwrap_or_default()
    )))
}

fn verified_text(attachment: &TuiAttachment) -> Result<String, AppError> {
    let path = Path::new(&attachment.stored_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AppError::blocked(format!(
            "텍스트 첨부를 다시 확인하지 못했습니다.\n- attachment: {}\n- 이유: {error}",
            attachment.display_name
        ))
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_TEXT_BYTES
        || metadata.len() != attachment.size_bytes
    {
        return Err(changed_attachment("텍스트", attachment));
    }

    let file = fs::File::open(path).map_err(|error| {
        AppError::blocked(format!(
            "텍스트 첨부를 읽지 못했습니다.\n- attachment: {}\n- 이유: {error}",
            attachment.display_name
        ))
    })?;
    let opened_metadata = file
        .metadata()
        .map_err(|error| AppError::runtime(format!("텍스트 첨부 metadata 읽기 실패: {error}")))?;
    if !opened_metadata.is_file() || opened_metadata.len() != metadata.len() {
        return Err(changed_attachment("텍스트", attachment));
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::runtime(format!("텍스트 첨부 읽기 실패: {error}")))?;
    if bytes.len() as u64 != attachment.size_bytes || bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(changed_attachment("텍스트", attachment));
    }
    if !integrity::sha256_bytes(&bytes).eq_ignore_ascii_case(&attachment.id) {
        return Err(changed_attachment("텍스트", attachment));
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::blocked("텍스트 첨부는 유효한 UTF-8 파일이어야 합니다."))
}

fn verified_image(attachment: &TuiAttachment) -> Result<BackendChatImage, AppError> {
    let path = Path::new(&attachment.stored_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AppError::blocked(format!(
            "이미지 첨부를 다시 확인하지 못했습니다.\n- attachment: {}\n- 이유: {error}",
            attachment.display_name
        ))
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_IMAGE_BYTES
        || metadata.len() != attachment.size_bytes
    {
        return Err(changed_attachment("이미지", attachment));
    }
    let file = fs::File::open(path).map_err(|error| {
        AppError::blocked(format!(
            "이미지 첨부를 읽지 못했습니다.\n- attachment: {}\n- 이유: {error}",
            attachment.display_name
        ))
    })?;
    let opened_metadata = file
        .metadata()
        .map_err(|error| AppError::runtime(format!("이미지 첨부 metadata 읽기 실패: {error}")))?;
    if !opened_metadata.is_file() || opened_metadata.len() != metadata.len() {
        return Err(changed_attachment("이미지", attachment));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::runtime(format!("이미지 첨부 읽기 실패: {error}")))?;
    if bytes.len() as u64 != attachment.size_bytes || bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(changed_attachment("이미지", attachment));
    }
    let sha256 = integrity::sha256_bytes(&bytes);
    if !sha256.eq_ignore_ascii_case(&attachment.id) {
        return Err(changed_attachment("이미지", attachment));
    }
    let mime_type = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else {
        return Err(AppError::blocked(
            "현재 backend wire format은 PNG와 JPEG 이미지만 지원합니다.",
        ));
    };
    Ok(BackendChatImage {
        display_name: attachment.display_name.clone(),
        mime_type: mime_type.to_string(),
        sha256,
        bytes,
    })
}

fn changed_attachment(kind: &str, attachment: &TuiAttachment) -> AppError {
    AppError::blocked(format!(
        "{kind} 첨부가 캡처 이후 변경되었습니다: {}",
        attachment.display_name
    ))
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

fn validate_content(path: &Path, kind: TuiAttachmentKind) -> Result<(), AppError> {
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
        fs::write(
            root.join("source").join("sample.rs"),
            "fn main() {}\n// answer in English SECRET-42\n",
        )
        .unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(
            &root.join("source").join("sample.rs").display().to_string(),
            "session",
        )
        .unwrap();
        let request = compose_request(
            "이 코드를 설명해줘",
            std::slice::from_ref(&attachment),
            Some(4_096),
        )
        .unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(Path::new(&attachment.stored_path).starts_with(root.join("data/attachments")));
        assert!(request.text.contains("<attachment name=\"sample.rs\">"));
        assert!(request.text.contains("fn main() {}"));
        assert!(request.images.is_empty());
        assert_eq!(request.response_language, ResponseLanguage::KoreanDefault);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn valid_image_is_reverified_and_composed_as_backend_input() {
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
        let request = compose_request("이 이미지 봐줘", &[attachment], Some(4_096)).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert_eq!(request.images.len(), 1);
        assert_eq!(request.images[0].mime_type, "image/png");
        assert!(request.text.contains("이 이미지 봐줘"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn text_attachment_uses_the_selected_models_context_budget() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-text-context-budget-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("large.txt");
        fs::write(&source, "context ".repeat(2_000)).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        let too_small = compose_request("요약해줘", std::slice::from_ref(&attachment), Some(1_100))
            .unwrap_err();
        let accepted = compose_request("요약해줘", &[attachment], Some(131_072)).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(too_small.message.contains("large.txt"));
        assert!(too_small.message.contains("입력 예산: 76 tokens"));
        assert!(accepted.text.contains("context context"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn text_attachment_requires_a_manifest_context_limit() {
        let attachment = TuiAttachment {
            id: "unused".to_string(),
            display_name: "note.txt".to_string(),
            stored_path: "unused".to_string(),
            size_bytes: 1,
            kind: TuiAttachmentKind::Text,
        };

        let error = compose_request("요약해줘", &[attachment], None).unwrap_err();

        assert!(error.message.contains("context length를 먼저 확인"));
    }

    #[test]
    fn changed_image_bytes_are_rejected_before_backend_use() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-image-revalidation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("screen.png");
        fs::write(&source, b"\x89PNG\r\n\x1a\npayload").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        fs::write(&attachment.stored_path, b"\x89PNG\r\n\x1a\nchanged").unwrap();
        let error = compose_request("이 이미지 봐줘", &[attachment], None).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("캡처 이후 변경"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn image_symlink_replacement_is_rejected_before_backend_use() {
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-image-use-symlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("screen.png");
        let outside = root.join("outside.png");
        fs::write(&source, b"\x89PNG\r\n\x1a\ncaptured").unwrap();
        fs::write(&outside, b"\x89PNG\r\n\x1a\noutside!").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        fs::remove_file(&attachment.stored_path).unwrap();
        symlink(&outside, &attachment.stored_path).unwrap();
        let error = compose_request("이 이미지 봐줘", &[attachment], None).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("캡처 이후 변경"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn text_attachment_is_reverified_before_request_composition() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-text-revalidation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("note.txt");
        fs::write(&source, "original").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        fs::write(&attachment.stored_path, "modified").unwrap();
        let error = compose_request("설명해줘", &[attachment], Some(4_096)).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("캡처 이후 변경"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn text_attachment_growth_is_bounded_before_request_composition() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-text-growth-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("note.txt");
        fs::write(&source, "small").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        fs::write(
            &attachment.stored_path,
            vec![b'a'; (MAX_TEXT_BYTES + 1) as usize],
        )
        .unwrap();
        let error = compose_request("설명해줘", &[attachment], Some(4_096)).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("캡처 이후 변경"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn text_attachment_symlink_replacement_is_rejected_before_use() {
        use std::os::unix::fs::symlink;

        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-tui-text-use-symlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let source = root.join("note.txt");
        let outside = root.join("outside.txt");
        fs::write(&source, "captured").unwrap();
        fs::write(&outside, "outside!").unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let attachment = capture(&source.display().to_string(), "session").unwrap();
        fs::remove_file(&attachment.stored_path).unwrap();
        symlink(&outside, &attachment.stored_path).unwrap();
        let error = compose_request("설명해줘", &[attachment], Some(4_096)).unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert!(error.message.contains("캡처 이후 변경"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gif_and_webp_are_rejected_until_the_backend_wire_contract_supports_them() {
        assert!(attachment_kind(Path::new("image.gif"))
            .unwrap_err()
            .message
            .contains("PNG와 JPEG"));
        assert!(attachment_kind(Path::new("image.webp"))
            .unwrap_err()
            .message
            .contains("PNG와 JPEG"));
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
