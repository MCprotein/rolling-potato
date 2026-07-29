use super::*;

pub(in crate::app::tui_adapter) fn capture(
    path_input: &str,
    session_id: &str,
) -> Result<TuiAttachment, AppError> {
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
    create_private_capture_dir(&capture_dir)?;
    let stored_path = capture_dir.join(format!("{}-{}", sha256, safe_leaf(&display_name)));
    match fs::symlink_metadata(&stored_path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(error) = copy_attachment_once(&source, &stored_path) {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(AppError::runtime(format!(
                        "첨부 파일을 app data에 캡처하지 못했습니다: {} ({error})",
                        stored_path.display()
                    )));
                }
            }
        }
        Err(error) => {
            return Err(AppError::runtime(format!(
                "첨부 저장 경로를 확인하지 못했습니다: {} ({error})",
                stored_path.display()
            )));
        }
    }
    harden_stored_attachment(&stored_path)?;
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

fn create_private_capture_dir(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|error| {
        AppError::runtime(format!(
            "첨부 저장소를 만들지 못했습니다: {} ({error})",
            path.display()
        ))
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AppError::runtime(format!(
            "첨부 저장소를 확인하지 못했습니다: {} ({error})",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::blocked(format!(
            "첨부 저장소를 차단했습니다.\n- path: {}\n- 이유: 일반 디렉터리만 사용할 수 있으며 symlink는 허용하지 않습니다.",
            path.display()
        )));
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        AppError::runtime(format!(
            "첨부 저장소 권한을 제한하지 못했습니다: {} ({error})",
            path.display()
        ))
    })?;
    Ok(())
}

fn copy_attachment_once(source: &Path, destination: &Path) -> std::io::Result<()> {
    let mut reader = File::open(source)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut writer = options.open(destination)?;
    let result = std::io::copy(&mut reader, &mut writer)
        .and_then(|_| writer.sync_all())
        .map(|_| ());
    drop(writer);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn harden_stored_attachment(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AppError::runtime(format!(
            "첨부 저장 경로를 다시 확인하지 못했습니다: {} ({error})",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "첨부 저장 경로를 차단했습니다.\n- path: {}\n- 이유: 기존 대상은 일반 파일이어야 하며 symlink는 허용하지 않습니다.",
            path.display()
        )));
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        AppError::runtime(format!(
            "첨부 파일 권한을 제한하지 못했습니다: {} ({error})",
            path.display()
        ))
    })?;
    Ok(())
}
