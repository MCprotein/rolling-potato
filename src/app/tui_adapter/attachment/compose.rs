use super::*;

pub(in crate::app::tui_adapter) fn compose_request(
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
        response_format: crate::runtime_core::inference::backend::BackendResponseFormat::Text,
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
    })?;
    let output_reserve = GenerationPolicyProfileV1::default()
        .prompt_output_reserve(limit)
        .map_err(|_| AppError::blocked("첨부 prompt generation capacity 부족"))?;
    let budget = PromptBudget::for_context_limit(limit as usize, output_reserve as usize)?;
    Ok(Some(budget.input_limit_tokens))
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
