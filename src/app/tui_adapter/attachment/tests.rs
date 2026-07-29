use super::*;

#[test]
fn captures_text_into_app_data_and_composes_a_bounded_request() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("rpotato-tui-attachment-{}", std::process::id()));
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
    #[cfg(unix)]
    {
        let file_mode = fs::metadata(&attachment.stored_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let directory_mode = fs::metadata(
            Path::new(&attachment.stored_path)
                .parent()
                .expect("captured attachment has a parent"),
        )
        .unwrap()
        .permissions()
        .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(directory_mode, 0o700);
    }
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

    let attachment = capture(&root.join("screen.png").display().to_string(), "session").unwrap();
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
    let too_small =
        compose_request("요약해줘", std::slice::from_ref(&attachment), Some(1_100)).unwrap_err();
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
    let root = std::env::temp_dir().join(format!("rpotato-tui-text-growth-{}", std::process::id()));
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
