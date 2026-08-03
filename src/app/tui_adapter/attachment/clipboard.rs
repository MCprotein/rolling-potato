//! Platform clipboard image intake without a package-manager dependency.

#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiAttachment;

#[cfg(target_os = "macos")]
const SCRIPT_OK: &str = "RPOTATO_CLIPBOARD_PNG_OK";
#[cfg(target_os = "macos")]
const SCRIPT_NO_PNG: &str = "RPOTATO_CLIPBOARD_NO_PNG";

#[cfg(target_os = "macos")]
pub(super) fn capture_clipboard_image(session_id: &str) -> Result<TuiAttachment, AppError> {
    capture_clipboard_image_with(session_id, run_osascript)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn capture_clipboard_image(session_id: &str) -> Result<TuiAttachment, AppError> {
    let temporary = ClipboardImage::read()?;
    super::capture(&temporary.path.display().to_string(), session_id)
}

#[cfg(target_os = "macos")]
fn capture_clipboard_image_with(
    session_id: &str,
    run_script: impl FnOnce(&Path, &str) -> std::io::Result<ScriptOutput>,
) -> Result<TuiAttachment, AppError> {
    let temporary = ClipboardImage::read_with(run_script)?;
    super::capture(&temporary.path.display().to_string(), session_id)
}

#[derive(Debug)]
struct ClipboardImage {
    path: PathBuf,
    #[cfg(target_os = "macos")]
    directory: PathBuf,
}

impl ClipboardImage {
    #[cfg(target_os = "macos")]
    fn read_with(
        run_script: impl FnOnce(&Path, &str) -> std::io::Result<ScriptOutput>,
    ) -> Result<Self, AppError> {
        let temporary = Self::create_private_staging_file()?;
        let script = clipboard_script(&temporary.path);
        let output = run_script(&temporary.path, &script).map_err(|error| {
            AppError::runtime(format!(
                "macOS 클립보드 이미지 변환기를 실행하지 못했습니다.\n- 이유: {error}"
            ))
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let outcome = stdout.trim();

        if output.success && outcome == SCRIPT_OK {
            return Ok(temporary);
        }
        if output.success && outcome == SCRIPT_NO_PNG {
            return Err(AppError::usage(
                "클립보드에 읽을 수 있는 PNG 이미지가 없습니다. 이미지 파일을 붙여넣거나 스크린샷을 복사한 뒤 Ctrl+V를 사용하세요.",
            ));
        }

        let diagnostic = sanitized_diagnostic(&output.stderr);
        let reason = if diagnostic.is_empty() {
            format!(
                "osascript가 예상하지 못한 결과를 반환했습니다 ({})",
                output.status
            )
        } else {
            format!("{} ({})", diagnostic, output.status)
        };
        Err(AppError::runtime(format!(
            "macOS 클립보드 이미지를 안전한 임시 파일에 저장하지 못했습니다.\n- 이유: {reason}"
        )))
    }

    #[cfg(target_os = "macos")]
    fn create_private_staging_file() -> Result<Self, AppError> {
        use std::fs::{self, DirBuilder, OpenOptions};
        use std::io::ErrorKind;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..32_u8 {
            let directory = std::env::temp_dir().join(format!(
                "rpotato-clipboard-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            let mut builder = DirBuilder::new();
            builder.mode(0o700);
            match builder.create(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AppError::runtime(format!(
                    "클립보드 이미지용 비공개 임시 디렉터리를 만들지 못했습니다.\n- 이유: {error}"
                )))
                }
            }
            if let Err(error) = fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)) {
                let _ = fs::remove_dir(&directory);
                return Err(AppError::runtime(format!(
                    "클립보드 이미지용 임시 디렉터리 권한을 제한하지 못했습니다.\n- 이유: {error}"
                )));
            }

            let path = directory.join("clipboard.png");
            let mut options = OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            match options.open(&path) {
                Ok(file) => {
                    drop(file);
                    if let Err(error) =
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                    {
                        let _ = fs::remove_file(&path);
                        let _ = fs::remove_dir(&directory);
                        return Err(AppError::runtime(format!(
                            "클립보드 이미지용 임시 파일 권한을 제한하지 못했습니다.\n- 이유: {error}"
                        )));
                    }
                    return Ok(Self { path, directory });
                }
                Err(error) => {
                    let _ = fs::remove_dir(&directory);
                    return Err(AppError::runtime(format!(
                        "클립보드 이미지용 임시 파일을 안전하게 만들지 못했습니다.\n- 이유: {error}"
                    )));
                }
            }
        }
        Err(AppError::runtime(
            "충돌하지 않는 클립보드 이미지 임시 경로를 만들지 못했습니다.",
        ))
    }

    #[cfg(not(target_os = "macos"))]
    fn read() -> Result<Self, AppError> {
        Err(AppError::usage(
            "이 플랫폼에서는 클립보드 원본 이미지 읽기를 아직 지원하지 않습니다. 이미지 파일을 복사하거나 끌어다 놓아 경로로 첨부하세요.",
        ))
    }
}

#[cfg(target_os = "macos")]
struct ScriptOutput {
    success: bool,
    status: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[cfg(target_os = "macos")]
fn run_osascript(_: &Path, script: &str) -> std::io::Result<ScriptOutput> {
    use std::process::Command;

    let output = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()?;
    Ok(ScriptOutput {
        success: output.status.success(),
        status: output.status.to_string(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

#[cfg(target_os = "macos")]
fn clipboard_script(path: &Path) -> String {
    let escaped_path = path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "try\nset png_data to (the clipboard as «class PNGf»)\non error error_message number error_number\nif error_number is -1700 then return \"{SCRIPT_NO_PNG}\"\nerror error_message number error_number\nend try\nset file_ref to open for access POSIX file \"{escaped_path}\" with write permission\ntry\nset eof file_ref to 0\nwrite png_data to file_ref\nclose access file_ref\nreturn \"{SCRIPT_OK}\"\non error error_message number error_number\ntry\nclose access file_ref\nend try\nerror error_message number error_number\nend try"
    )
}

#[cfg(target_os = "macos")]
fn sanitized_diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .take(512)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Drop for ClipboardImage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        #[cfg(target_os = "macos")]
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn output(success: bool, stdout: &str, stderr: &str) -> ScriptOutput {
        ScriptOutput {
            success,
            status: if success {
                "exit status: 0"
            } else {
                "exit status: 1"
            }
            .to_string(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn clipboard_adapter_captures_private_staging_file_and_cleans_it() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-clipboard-adapter-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        let mut staged = None;

        let attachment = capture_clipboard_image_with("session", |path, script| {
            assert!(script.contains(SCRIPT_OK));
            let directory = path.parent().unwrap();
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            staged = Some((path.to_path_buf(), directory.to_path_buf()));
            fs::write(path, b"\x89PNG\r\n\x1a\nfixture")?;
            Ok(output(true, SCRIPT_OK, ""))
        })
        .expect("clipboard fixture should be captured");

        std::env::remove_var("RPOTATO_DATA_HOME");
        assert_eq!(
            attachment.kind,
            crate::surfaces::tui::runtime_bridge::TuiAttachmentKind::Image
        );
        assert!(Path::new(&attachment.stored_path).is_file());
        let (path, directory) = staged.unwrap();
        assert!(!path.exists());
        assert!(!directory.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_clipboard_png_is_reported_without_leaking_staging_files() {
        let mut staged = None;
        let error = capture_clipboard_image_with("session", |path, _| {
            staged = Some((path.to_path_buf(), path.parent().unwrap().to_path_buf()));
            Ok(output(true, SCRIPT_NO_PNG, ""))
        })
        .unwrap_err();

        assert_eq!(error.code, 2);
        assert!(error.message.contains("PNG 이미지가 없습니다"));
        let (path, directory) = staged.unwrap();
        assert!(!path.exists());
        assert!(!directory.exists());
    }

    #[test]
    fn clipboard_write_failure_keeps_sanitized_evidence_and_cleans_up() {
        let mut staged = None;
        let error = capture_clipboard_image_with("session", |path, _| {
            staged = Some((path.to_path_buf(), path.parent().unwrap().to_path_buf()));
            Ok(output(false, "", "Disk FULL\u{0000}\npermission DENIED"))
        })
        .unwrap_err();

        assert_eq!(error.code, 1);
        assert!(error.message.contains("Disk FULL permission DENIED"));
        assert!(!error.message.contains('\u{0000}'));
        let (path, directory) = staged.unwrap();
        assert!(!path.exists());
        assert!(!directory.exists());
    }
}
